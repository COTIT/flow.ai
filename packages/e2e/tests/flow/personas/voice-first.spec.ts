// Persona 7, the voice-first talker: talks over the agent, mixes voice and typing, fiddles with
// the microphone (denied, granted, revoked, muted, another device), says "yes" to review cards,
// asks for chart changes out loud, keeps talking across scenes, goes quiet, hangs up mid-tool.
// SIMULATED: fake microphone (Chromium fake media), mocked voice socket and API (see harness.ts).
import type { Page, Request } from "@playwright/test"

import { readBack, scene, stockRoutes } from "../task"
import { v } from "../voice"
import { expect, test, type Persona } from "./harness"

test.use({
  permissions: ["microphone"],
  launchOptions: { args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"] },
})

const CODE = "135790"
const PHRASE = `Confirm action ${CODE}`

/** Every decision the browser sent, with how it was made (click or speech). */
function decisions(page: Page) {
  const seen: { ticket: string; decision: string; method?: string; utterance?: string; eventId?: string }[] = []
  page.on("request", (r: Request) => {
    if (new URL(r.url()).pathname !== "/api/voice/confirm") return
    try {
      seen.push(r.postDataJSON())
    } catch {
      seen.push({ ticket: "?", decision: "unreadable" })
    }
  })
  return seen
}

/**
 * The microphone under the person's control: deny the next request, or revoke (stop) every live
 * track mid-turn, as the OS or another app would. Counts requests.
 */
async function micControl(page: Page) {
  await page.addInitScript(() => {
    const w = window as unknown as { __mic: { deny: boolean; requests: number; tracks: MediaStreamTrack[] } }
    w.__mic = { deny: false, requests: 0, tracks: [] }
    const md = navigator.mediaDevices
    const real = md?.getUserMedia?.bind(md)
    if (!real) return
    md.getUserMedia = async (c) => {
      w.__mic.requests++
      if (w.__mic.deny) throw new DOMException("Permission denied", "NotAllowedError")
      const stream = await real(c)
      w.__mic.tracks.push(...stream.getAudioTracks())
      return stream
    }
  })
  return {
    deny: (on: boolean) => page.evaluate((d) => ((window as unknown as { __mic: { deny: boolean } }).__mic.deny = d), on),
    /** The OS takes the mic away: tracks end (fires "ended"). */
    revoke: () =>
      page.evaluate(() => {
        const m = (window as unknown as { __mic: { tracks: MediaStreamTrack[] } }).__mic
        for (const t of m.tracks) {
          t.stop()
          t.dispatchEvent(new Event("ended"))
        }
      }),
    deviceChange: () => page.evaluate(() => navigator.mediaDevices.dispatchEvent(new Event("devicechange"))),
  }
}

/** What the person says (the agent's ASR hears it): a final transcript with its own event id. */
let nextId = 50_000
function speak(persona: Persona, words: string, { id = nextId++, interrupt = false } = {}) {
  if (interrupt) persona.agent.emit({ type: "interruption", interruption_event: { event_id: id } })
  persona.agent.emit({ type: "user_transcript", user_transcription_event: { user_transcript: words, event_id: id } })
  // The agent answers what it heard (the harness agent answers only typed lines).
  setTimeout(
    () => persona.agent.emit({ type: "agent_response", agent_response_event: { agent_response: `Heard: ${words.slice(0, 40)}`, event_id: id + 1 } }),
    120
  )
  return id
}

async function proposeStock(persona: Persona, page: Page, total = 120) {
  persona.agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: total }] },
  })
  await expect(scene(page)).toContainText(PHRASE)
}

async function setup(page: Page, api: import("../../office/fake-api").FakeApi) {
  const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
  return stockRoutes(page)
}

async function startVoice(page: Page) {
  await page.getByRole("button", { name: v.start, exact: true }).click()
  await expect(page.getByRole("button", { name: v.end })).toBeVisible()
  // The dock's attach button exists only once the session is verified.
  await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
}

// ---- probes ------------------------------------------------------------------------------------

for (const words of ["yes", "Yes, approve it", "approve", "confirm", `${PHRASE} please`, `don't ${PHRASE.toLowerCase()}`, "Confirm action one three five seven nine zero"])
  test(`probe: saying "${words}" to a review card never approves`, async ({ persona, page, api }) => {
    const sent = decisions(page)
    await setup(page, api)
    await page.goto("/")
    await startVoice(page)
    await proposeStock(persona, page)
    speak(persona, words, { interrupt: persona.chance(0.5) })
    await page.waitForTimeout(1200)
    console.log(`[probe] "${words}":`, JSON.stringify(sent))
    expect(sent.filter((d) => d.decision === "approve")).toEqual([])
    await persona.check()
  })

test("probe: the agent reads the phrase itself; a transcript without id; a replayed id", async ({ persona, page, api }) => {
  const sent = decisions(page)
  await setup(page, api)
  await page.goto("/")
  await startVoice(page)
  await proposeStock(persona, page)
  persona.agent.say(`Please say: ${PHRASE}`)
  persona.agent.say(PHRASE)
  persona.agent.emit({ type: "agent_response", agent_response_event: { agent_response: PHRASE, event_id: 77 } })
  await page.waitForTimeout(800)
  console.log("[probe] agent reads phrase:", JSON.stringify(sent))
  persona.agent.emit({ type: "user_transcript", user_transcription_event: { user_transcript: PHRASE } })
  await page.waitForTimeout(800)
  console.log("[probe] transcript without id:", JSON.stringify(sent), await scene(page).count())
  expect(sent.filter((d) => d.decision === "approve")).toEqual([])
  // A real agent answers what it heard; the fake one does so here (else "Thinking…" waits forever).
  persona.agent.emit({ type: "agent_response", agent_response_event: { agent_response: "I heard you.", event_id: 78 } })
  await persona.check()
})

test("probe: the exact phrase approves once, even said twice or with the same id again", async ({ persona, page, api }) => {
  const sent = decisions(page)
  await setup(page, api)
  await page.goto("/")
  await startVoice(page)
  await proposeStock(persona, page)
  const id = speak(persona, PHRASE, { interrupt: true })
  speak(persona, PHRASE, { id })
  speak(persona, PHRASE)
  await page.waitForTimeout(1500)
  console.log("[probe] phrase twice:", JSON.stringify(sent))
  expect(sent.filter((d) => d.decision === "approve")).toHaveLength(1)
  await persona.check()
})

test("probe: mic denied, granted, revoked mid-turn, device change, mute spam", async ({ persona, page }) => {
  const mic = await micControl(page)
  await page.goto("/")
  await mic.deny(true)
  await persona.click(page.getByRole("button", { name: v.start, exact: true }))
  await page.waitForTimeout(1500)
  console.log("[probe] denied:", await page.locator("[data-phase]").first().getAttribute("data-phase"), await page.getByRole("alert").allInnerTexts(), await page.getByRole("status").allInnerTexts())
  await persona.check()
  await mic.deny(false)
  await persona.click(page.getByRole("button", { name: v.start, exact: true }))
  await page.waitForTimeout(1500)
  console.log("[probe] granted:", await page.locator("[data-phase]").first().getAttribute("data-phase"), await page.getByRole("alert").allInnerTexts())
  speak(persona, "What were sales today?")
  await mic.revoke()
  await page.waitForTimeout(1200)
  console.log("[probe] revoked:", await page.locator("[data-phase]").first().getAttribute("data-phase"), await page.getByRole("alert").allInnerTexts(), await page.getByRole("status").allInnerTexts(), await page.getByText(v.capturePaused).count())
  await mic.deviceChange()
  const micButtons = new RegExp(`^(${v.mute}|${v.unmute}|${v.openMic}|Microphone)$`)
  for (let i = 0; i < 8; i++) await persona.click(page.getByRole("button", { name: micButtons }).nth(persona.int(0, 1)))
  await page.waitForTimeout(800)
  console.log("[probe] after spam:", await page.getByRole("status").allInnerTexts(), await page.getByRole("alert").allInnerTexts())
  await persona.check()
})

// ---- findings (red until fixed; delete test.fail() to see the real failure) --------------------

test.describe("P7-1: a refused microphone shows the browser's raw text", () => {
  // Fixed: regression guard.
  test("P7-1: Start voice with the microphone refused", async ({ persona, page }) => {
    const mic = await micControl(page)
    await page.goto("/")
    await mic.deny(true)
    await persona.click(page.getByRole("button", { name: v.start, exact: true }))
    await expect(page.getByRole("alert").first()).not.toBeEmpty()
    // Chrome says "Permission denied", Safari "The request is not allowed…": neither is Flow's words.
    await expect(page.getByRole("alert").first()).not.toHaveText("Permission denied")
    await expect(page.getByRole("alert").first()).toContainText(/microphone/i)
  })
})

test.describe("P7-2: the microphone taken away mid-call goes unnoticed", () => {
  // Fixed by 8cc908a48 (capture watcher): kept as a regression guard.
  test("P7-2: tracks end (unplugged / OS revoked); Flow keeps listening to nothing", async ({ persona, page }) => {
    const mic = await micControl(page)
    await page.goto("/")
    await startVoice(page)
    speak(persona, "What were sales today?")
    await mic.revoke()
    // Expected: voice pauses and says so (the existing "Microphone paused" / Resume voice path).
    await expect(page.getByText(v.capturePaused).first()).toBeVisible({ timeout: 5_000 })
  })
})

// ---- the walk ----------------------------------------------------------------------------------

test.describe("walk", () => {
  // P6-4 (reported): the SDK logs these when a tool answers after the socket closed.
  test.use({
    allow: {
      consolePrefix: [
        "Client tool execution failed with following error: WebSocket is already in CLOSING or CLOSED state.",
        "Unexpected error in client tool call handling: WebSocket is already in CLOSING or CLOSED state.",
      ],
    },
  })
/** Steps per walk: PERSONA_STEPS wins; CI runs a short walk (suite budget), local runs the full one. */
const STEPS = Number(process.env.PERSONA_STEPS) || (process.env.CI ? 12 : 40)

test("voice-first talker: barge-in, voice+text, mic flips, spoken yes, scenes, silence, hang-ups", async ({ persona, page, api }) => {
  const sent = decisions(page)
  await setup(page, api)
  const mic = await micControl(page)
  await page.emulateMedia({ colorScheme: persona.chance(0.5) ? "dark" : "light" })
  /** Event ids of exact confirmation phrases the person really said (the only speech approvals allowed). */
  const saidPhrase = new Set<string>()
  const replies: string[] = []
  let slowTools = false
  await page.route(/\/api\/voice\/tools/, async (route) => {
    if (slowTools) await new Promise((r) => setTimeout(r, 2500))
    await route.fallback().catch(() => {})
  })
  const live = () => page.getByRole("button", { name: v.addFiles }).isVisible()
  const talk = (words: string, interrupt = persona.chance(0.4)) => {
    const id = speak(persona, words, { interrupt })
    replies.push(`Heard: ${words.slice(0, 40)}`)
    return id
  }
  await page.goto("/")
  // A real agent speaks after its tool results (P7-3 covers the silent case).
  let answered = 0
  const speakAfterTools = <T extends { run: () => Promise<unknown> }>(step: T): T => ({
    ...step,
    run: async () => {
      await step.run()
      await page.waitForTimeout(50)
      const n = persona.agent.results()
      if (n > answered) persona.agent.say(`Done (${n}).`)
      answered = n
    },
  })
  await persona.run([
    { name: "start voice", weight: 3, run: () => persona.click(page.getByRole("button", { name: v.start, exact: true })) },
    { name: "start text", weight: 1, run: () => persona.startText() },
    {
      name: "talk",
      weight: 6,
      run: async () => {
        if (!(await live())) return
        talk(persona.pick(["What were sales today?", "Which orders wait?", "Show stock below minimum", "make it a pie", "show it as a table instead", "Thanks", "hmm", "Wacha nione mauzo"]))
      },
    },
    {
      name: "barge in while the agent speaks",
      weight: 3,
      run: async () => {
        if (!(await live())) return
        persona.agent.say("Sales today are twelve thousand eight hundred and forty euros across fourteen orders, which is more than yesterday, and")
        await page.waitForTimeout(persona.int(0, 200))
        talk(persona.pick(["wait", "no, this week", "stop"]), true)
      },
    },
    { name: "type while in voice", weight: 2, run: () => persona.say(persona.pick(["also check orders", "and the honey stock?"])) },
    {
      name: "agent proposes a stock change",
      weight: 3,
      run: async () => {
        if (!(await live())) return
        persona.agent.call("stock_set_levels", {
          path: { product: "p-500" },
          body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: persona.pick([110, 120, 130]) }] },
        })
      },
    },
    {
      name: "say yes to the card",
      weight: 3,
      run: async () => {
        if (!(await live())) return
        talk(persona.pick(["yes", "approve it", "Yes please go ahead", "confirm", "ok do it"]))
      },
    },
    {
      name: "read the phrase from the card",
      weight: 2,
      run: async () => {
        if (!(await live()) || !(await scene(page).getByText(PHRASE).count())) return
        saidPhrase.add(String(talk(PHRASE)))
      },
    },
    { name: "agent reads the phrase itself", weight: 1, run: async () => persona.agent.say(PHRASE) },
    { name: "approve by click", weight: 2, run: () => persona.click(scene(page).getByRole("button", { name: v.approve })) },
    {
      name: "silence check-ins",
      weight: 2,
      run: async () => {
        if (!(await live())) return
        for (const line of ["Still there?", "Take your time, I am listening.", "I'll end the call now. Bye!"].slice(0, persona.int(1, 3))) {
          persona.agent.say(line)
          await page.waitForTimeout(300)
        }
      },
    },
    {
      name: "hang up mid-tool, then restart",
      weight: 2,
      run: async () => {
        if (!(await live())) return
        slowTools = true
        persona.agent.call("products_list", {})
        await page.waitForTimeout(persona.int(100, 800))
        await persona.click(page.getByRole("button", { name: v.end }))
        await page.waitForTimeout(persona.int(0, 600))
        slowTools = false
        if (persona.chance(0.7)) await persona.click(page.getByRole("button", { name: v.start, exact: true }))
      },
    },
    { name: "the agent's socket drops", weight: 1, run: async () => persona.agent.close() },
    { name: "mic refused next time", weight: 1, run: () => mic.deny(true) },
    { name: "mic allowed again", weight: 2, run: () => mic.deny(false) },
    { name: "mic taken away", weight: 1, run: () => mic.revoke() },
    { name: "audio device changes", weight: 1, run: () => mic.deviceChange() },
    {
      name: "mic button spam",
      weight: 2,
      run: async () => {
        const buttons = page.getByRole("button", { name: new RegExp(`^(${v.mute}|${v.unmute}|${v.openMic}|Microphone|${v.resumeVoice}|${v.enableVoice})$`) })
        for (let i = persona.int(2, 8); i > 0; i--) await persona.click(buttons.nth(persona.int(0, 2)))
      },
    },
    {
      name: "open a scene while talking",
      weight: 2,
      run: async () => {
        await persona.backHome()
        const links = page.getByRole("region", { name: /Your Flow/ }).getByRole("link")
        const n = await links.count()
        if (n) await persona.click(links.nth(persona.int(0, n - 1)))
      },
    },
    { name: "back home", weight: 1, run: () => persona.backHome() },
    { name: "hold space to talk", weight: 1, run: async () => {
      await page.keyboard.down("Space")
      await page.waitForTimeout(persona.int(50, 600))
      await page.keyboard.up("Space")
    } },
    {
      name: "invariants: speech approvals, one reply per line",
      weight: 2,
      run: async () => {
        for (const d of sent.filter((x) => x.decision === "approve" && x.method === "speech")) {
          expect(d.utterance?.toLowerCase(), "a spoken approval that was not the exact phrase").toBe(PHRASE.toLowerCase())
          expect(saidPhrase.has(String(d.eventId)), "a speech approval nobody said").toBe(true)
        }
        const log = await page.getByRole("log").first().innerText({ timeout: 1000 }).catch(() => "")
        for (const r of new Set(replies)) {
          const n = log.split(r).length - 1
          expect(n, `reply shown ${n} times: ${r}`).toBeLessThanOrEqual(replies.filter((x) => x === r).length)
        }
      },
    },
  ].map(speakAfterTools), { count: STEPS })
})
})

test.describe("P7-3: an agent tool call outside a user turn, then silence: Thinking… forever", () => {
  // Fixed: regression guard.
  test("P7-3: the agent runs a read on its own and says nothing after it", async ({ persona, page }) => {
    await page.clock.install()
    await page.goto("/")
    await page.getByRole("button", { name: v.textInstead, exact: true }).click()
    await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
    // E.g. the status wait the agent starts after a review card, or a read after a check-in.
    persona.agent.call("products_list", {})
    await expect.poll(() => persona.agent.results()).toBe(1)
    await page.clock.runFor(60_000)
    // Inside a user turn the same silence ends with "I couldn't finish that answer" after 45 s.
    await expect(page.locator('[aria-busy="true"]')).toHaveCount(0)
  })
})
