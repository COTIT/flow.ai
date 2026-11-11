// Persona 5, the multitasker: two or three tabs on one login. Approves in one tab while another
// shows the same card, talks in both, switches workspace or language in one, logs out in one
// while another works, reloads or closes a tab mid-request, opens the same scene everywhere.
// SIMULATED API and voice (see harness.ts): each tab has its own fake agent; Office's tickets are
// one-shot in the mock (a second decision is refused), so the UI must never send one.
import type { Page } from "@playwright/test"

import { RUAHA, ILUNDO } from "../api"
import { sc } from "../scenes-api"
import { readBack } from "../task"
import { expect, test, type Persona } from "./harness"
import {
  allowAborts,
  approve,
  backHome,
  cancel,
  expectOneLanguage,
  Ledger,
  live,
  LOGIN,
  logOut,
  multitaskRoutes,
  openTab,
  say,
  savedWorkspace,
  setLanguage,
  startText,
  switchWorkspace,
  L,
  type Locale,
  type Tab,
} from "./multitasker.helpers"

const NAMES: Record<string, string> = { w1: ILUNDO.display_name, w2: RUAHA.display_name }
const other = (ws: string | null) => (ws === "w2" ? "w1" : "w2")

async function setup(persona: Persona, page: Page, api: import("../../office/fake-api").FakeApi) {
  const ledger = new Ledger()
  for (const ws of ["w1", "w2"]) {
    const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
    api.on("GET", `/workspace/${ws}/products/p-500/stock-with-bins`, () => stock.body as object)
  }
  api.on("POST", "/login", () => ({
    data: {
      access_token: "e2e-token",
      token_type: "Bearer",
      expires_at: "2099-01-01T00:00:00Z",
      workspace_ids: ["w1", "w2"],
    },
  }))
  await multitaskRoutes(page, ledger, 0)
  await page.goto("/")
  const tabs: Tab[] = [{ page, agent: persona.agent, n: 0 }]
  /** Keeps the foreign list in step: the workspace not shown now must leave no trace anywhere. */
  const syncForeign = async () => {
    const ws = await savedWorkspace(page)
    const foreign = persona.options.foreign!
    foreign.splice(0, foreign.length, ...(ws ? (ledger.markers[other(ws)] ?? []) : []))
  }
  return { ledger, tabs, syncForeign }
}

test("multitasker: two or three tabs on one login, random walk", async ({ persona, page, api }) => {
  const { ledger, tabs, syncForeign } = await setup(persona, page, api)
  await page.emulateMedia({ colorScheme: persona.chance(0.5) ? "dark" : "light" })
  let opened = 0
  const open = () => tabs.filter((t) => !t.page.isClosed())
  const anyTab = () => persona.pick(open())
  const otherTab = (t: Tab) => persona.pick(open().filter((x) => x !== t).concat(open().length > 1 ? [] : [t]))
  const ask = async (t: Tab) => {
    const ws = (await savedWorkspace(t.page)) ?? "w?"
    const marker = ledger.mark(ws, `${ws.toUpperCase()}Q${ledger.seq++}x`)
    return say(t.page, `${persona.pick(["Sales today?", "Which orders wait?", "Stock below minimum"])} ${marker}`, persona.chance(0.6))
  }
  const propose = async (t: Tab) => {
    if (!(await live(t.page))) await startText(t.page)
    if (!(await live(t.page))) return
    t.agent.call("stock_set_levels", {
      path: { product: "p-500" },
      body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: persona.pick([110, 120, 130]) }] },
    })
  }
  const wrap = (fn: () => Promise<unknown>) => async () => {
    const t0 = Date.now()
    await fn()
    if (process.env.PERSONA_VERBOSE) console.log(`[timing] ${persona.trail.at(-1)} ${Date.now() - t0}ms`)
    await syncForeign()
    for (const t of open()) await expectOneLanguage(t.page)
    expect(ledger.approvedTwice(), "an action was approved twice (across tabs)").toEqual([])
  }

  await persona.run(
    [
      {
        name: "open another tab (same login)",
        weight: 2,
        run: wrap(async () => {
          if (open().length >= 3) return
          tabs.push(await openTab(persona, api, ledger, ++opened))
        }),
      },
      {
        name: "duplicate a tab (sessionStorage copied)",
        weight: 1,
        run: wrap(async () => {
          if (open().length >= 3) return
          const from = anyTab()
          tabs.push(await openTab(persona, api, ledger, ++opened, { url: from.page.url(), duplicateOf: from.page, session: from.n }))
        }),
      },
      { name: "start typing in a tab", weight: 2, run: wrap(() => startText(anyTab().page)) },
      { name: "ask in a tab", weight: 4, run: wrap(() => ask(anyTab())) },
      { name: "a stock change to review in a tab", weight: 3, run: wrap(() => propose(anyTab())) },
      { name: "approve in a tab", weight: 3, run: wrap(() => approve(anyTab().page, persona.chance(0.2) ? 2 : 1)) },
      {
        name: "approve here while the other tab reviews",
        weight: 2,
        run: wrap(async () => {
          const a = anyTab()
          const b = otherTab(a)
          await approve(a.page)
          await b.page.bringToFront()
          await approve(b.page)
        }),
      },
      { name: "cancel in a tab", weight: 1, run: wrap(() => cancel(anyTab().page)) },
      {
        name: "approve slowly here, switch workspace there",
        weight: 2,
        run: wrap(async () => {
          const a = anyTab()
          const b = otherTab(a)
          ledger.confirmDelay = persona.int(200, 1500)
          allowAborts(a.page)
          await approve(a.page)
          await switchWorkspace(b.page, NAMES[other(await savedWorkspace(b.page))]!)
          await a.page.waitForTimeout(ledger.confirmDelay)
          ledger.confirmDelay = 0
        }),
      },
      {
        name: "same scene in every tab",
        weight: 2,
        run: wrap(async () => {
          const first = open()[0]!.page
          await backHome(first)
          const links = first.getByRole("region", { name: sc.yourFlow }).getByRole("link")
          const n = await links.count()
          if (!n) return
          const href = await links.nth(persona.int(0, n - 1)).getAttribute("href")
          if (href) for (const t of open()) await t.page.goto(href).catch(() => {})
        }),
      },
      { name: "back home in a tab", weight: 2, run: wrap(() => backHome(anyTab().page)) },
      {
        name: "reload a tab mid-task",
        weight: 2,
        run: wrap(async () => {
          const t = anyTab()
          allowAborts(t.page)
          await t.page.reload().catch(() => {})
        }),
      },
      {
        name: "switch workspace in one tab",
        weight: 2,
        run: wrap(async () => {
          const t = anyTab()
          await switchWorkspace(t.page, NAMES[other(await savedWorkspace(t.page))]!)
        }),
      },
      {
        name: "approve slowly, then close that tab mid-request",
        weight: 1,
        run: wrap(async () => {
          if (open().length < 2) return
          const t = persona.pick(open().slice(1))
          await propose(t)
          ledger.confirmDelay = 600
          allowAborts(t.page)
          await approve(t.page)
          await t.page.close().catch(() => {})
          ledger.confirmDelay = 0
        }),
      },
      {
        name: "language in one tab, work in another, back to English",
        weight: 1,
        run: wrap(async () => {
          const a = anyTab()
          const to = persona.pick<Locale>(["nl", "sw"])
          await setLanguage(a.page, to)
          await expect(a.page.locator("html")).toHaveAttribute("lang", to)
          await expectOneLanguage(a.page)
          const b = otherTab(a)
          await ask(b)
          await backHome(b.page)
          for (const t of open()) await expectOneLanguage(t.page)
          await setLanguage(a.page, "en")
          await expect(a.page.locator("html")).toHaveAttribute("lang", "en")
          for (const t of open())
            if ((await t.page.evaluate(() => document.documentElement.lang).catch(() => "en")) !== "en") {
              allowAborts(t.page)
              await t.page.reload().catch(() => {})
            }
        }),
      },
      {
        name: "log out in one tab while another works, then log in again",
        weight: 1,
        run: wrap(async () => {
          const a = anyTab()
          const b = otherTab(a)
          await ask(b)
          allowAborts(b.page)
          if (!(await logOut(a.page))) return
          for (const t of open()) await expect(t.page).toHaveURL(LOGIN)
          await a.page.getByLabel("Email").fill("e2e@genesy.test")
          await a.page.getByLabel("Password").fill("e2e-password-only")
          await a.page.getByRole("button", { name: "Log in" }).click()
          await expect(a.page).not.toHaveURL(LOGIN)
          // The other tabs follow the new login (or stay on a working login page).
          for (const t of open()) if (t !== a) await t.page.goto("/").catch(() => {})
        }),
      },
    ],
    {}
  )
  console.log(
    `[multitasker] tabs=${tabs.length} reviews=${ledger.reviews.size} decisions=${ledger.decisions.length} ` +
      `restores=${ledger.restores.length} markers=${JSON.stringify(Object.fromEntries(Object.entries(ledger.markers).map(([k, v]) => [k, v.length])))}`
  )
})

// ---- targeted: one scenario per multitasking hazard ---------------------------------------
async function proposeIn(t: Tab, total = 120) {
  // Clicks again until connected (the first click can land before the page is ready).
  await expect
    .poll(async () => (await live(t.page)) || ((await startText(t.page)) && false), { timeout: 10_000 })
    .toBe(true)
  t.agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: total }] },
  })
  await expect(t.page.getByRole("button", { name: L.en.approve })).toBeVisible()
}

test.describe("targeted", () => {
  test("duplicate tab with a review open: approve in one, the other never approves it again", async ({ persona, page, api }) => {
    const { ledger, tabs } = await setup(persona, page, api)
    await proposeIn(tabs[0]!)
    const dup = await openTab(persona, api, ledger, 1, { url: page.url(), duplicateOf: page, session: 0 })
    await dup.page.waitForTimeout(1000)
    console.log("[probe] dup body:", (await dup.page.locator("main").innerText()).slice(0, 400))
    console.log("[probe] restores", ledger.restores, "dup url", dup.page.url())
    await approve(page)
    await expect(page.locator("[data-receipt]")).toBeVisible()
    await dup.page.bringToFront()
    await dup.page.waitForTimeout(500)
    const offered = await dup.page.getByRole("button", { name: L.en.approve }).isVisible()
    console.log("[probe] dup still offers approve:", offered)
    if (offered) await approve(dup.page)
    await dup.page.waitForTimeout(500)
    expect(ledger.approvedTwice(), "approved twice across tabs").toEqual([])
    await persona.check()
  })

  test("workspace switch in one tab while the other has a review open", async ({ persona, page, api }) => {
    const { ledger, tabs, syncForeign } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    await proposeIn(b)
    await proposeIn(tabs[0]!, 130)
    await switchWorkspace(page, RUAHA.display_name)
    await expect.poll(() => savedWorkspace(b.page)).toBe("w2")
    await syncForeign()
    await persona.check()
    // The old workspace's card is gone in both tabs, and nothing of w1 can be approved.
    for (const p of [page, b.page]) await expect(p.getByRole("button", { name: L.en.approve })).toHaveCount(0)
    expect(ledger.decisions).toEqual([])
  })

  test("log out in one tab while the other is approving (slow confirm)", async ({ persona, page, api }) => {
    const { ledger } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    await proposeIn(b)
    ledger.confirmDelay = 1500
    allowAborts(b.page)
    await approve(b.page)
    await logOut(page)
    for (const p of [page, b.page]) await expect(p).toHaveURL(LOGIN)
    await b.page.waitForTimeout(2000)
    await persona.check()
    console.log("[probe] decisions", ledger.decisions)
  })

  test("language switch in one tab, then the other tab moves around", async ({ persona, page, api }) => {
    const { ledger } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    await startText(b.page)
    await say(b.page, "Hello there")
    await setLanguage(page, "sw")
    await expect(page.locator("html")).toHaveAttribute("lang", "sw")
    await expectOneLanguage(page)
    // Tab B, untouched, keeps working: a scene and back, a message.
    const links = b.page.getByRole("region", { name: sc.yourFlow }).getByRole("link")
    if (await links.count()) await links.first().click()
    await b.page.waitForTimeout(800)
    await backHome(b.page)
    await b.page.waitForTimeout(800)
    console.log("[probe] B lang", await b.page.evaluate(() => document.documentElement.lang))
    await expectOneLanguage(b.page)
    await persona.check().catch((e) => console.log("[probe] harness check (english labels):", String(e).slice(0, 200)))
  })

  test("reload one tab with a review open; the other tab's review is untouched", async ({ persona, page, api }) => {
    const { ledger, tabs } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    await proposeIn(tabs[0]!)
    await proposeIn(b, 130)
    allowAborts(page)
    await page.reload()
    await page.waitForTimeout(1500)
    console.log("[probe] A after reload:", (await page.locator("main").innerText()).slice(0, 400))
    console.log("[probe] restores", ledger.restores, "A approve visible", await page.getByRole("button", { name: L.en.approve }).isVisible())
    await expect(b.page.getByRole("button", { name: L.en.approve })).toBeVisible()
    await approve(b.page)
    await expect(b.page.locator("[data-receipt]")).toBeVisible()
    await persona.check()
  })

  test("close a tab mid-approve; the other tab carries on", async ({ persona, page, api }) => {
    const { ledger } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    await proposeIn(b)
    ledger.confirmDelay = 1000
    allowAborts(b.page)
    await approve(b.page)
    await b.page.close()
    ledger.confirmDelay = 0
    await startText(page)
    await say(page, "Still here?")
    await expect(page.getByRole("log")).toContainText("Noted: Still here?")
    await persona.check()
  })

  test("same scene in three tabs, receipt in one, home in all", async ({ persona, page, api }) => {
    const { ledger, tabs } = await setup(persona, page, api)
    tabs.push(await openTab(persona, api, ledger, 1), await openTab(persona, api, ledger, 2))
    await proposeIn(tabs[1]!)
    await approve(tabs[1]!.page)
    await expect(tabs[1]!.page.locator("[data-receipt]")).toBeVisible()
    const href = await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().getAttribute("href")
    for (const t of tabs) await t.page.goto(href!)
    await persona.check()
    for (const t of tabs) await backHome(t.page)
    await persona.check()
    // Only the approving tab has the receipt; no other tab got it (sessionStorage per tab).
    await expect(tabs[0]!.page.locator("[data-receipt]")).toHaveCount(0)
    await expect(tabs[2]!.page.locator("[data-receipt]")).toHaveCount(0)
  })
})

test.describe("findings", () => {
  // P5-1: a reload while a review waits loses it. Office still lists the action as awaiting_review
  // and recoverable (the tab asks, `tools` status), but the resume prompt only shows after a
  // conversation *ended* (phase "ended"); after a reload the phase is "ready", so the review,
  // its Approve and "Continue review" are all unreachable until it silently expires.
  test("P5-1 reload with a review waiting: the review (or Continue review) stays reachable", async ({ persona, page, api }) => {
    const { ledger, tabs } = await setup(persona, page, api)
    await proposeIn(tabs[0]!)
    allowAborts(page)
    await page.reload()
    await expect.poll(() => ledger.restores.includes("0:status")).toBe(true)
    const card = page.getByRole("button", { name: L.en.approve })
    const resume = page.getByRole("button", { name: "Continue review" })
    await expect(card.or(resume).first()).toBeVisible({ timeout: 5_000 })
  })

  test("P5-1 the resume prompt never shows while a new conversation is starting", async ({ persona, page, api }) => {
    const { tabs } = await setup(persona, page, api)
    await proposeIn(tabs[0]!)
    allowAborts(page)
    await page.reload()
    const resume = page.getByRole("button", { name: "Continue review" })
    await expect(resume).toBeVisible({ timeout: 5_000 })
    // Hold the new session open so "connecting" lasts long enough to look at.
    await page.route("**/api/voice/session", async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 1_500))
      await route.fallback()
    })
    // A reload reopens the task view (F2); conversations start from home.
    await backHome(page)
    await expect(resume).toBeVisible()
    const started = startText(page)
    await expect(resume).toHaveCount(0, { timeout: 1_000 })
    await started
    await expect(resume).toHaveCount(0)
  })
})

test.describe("races", () => {
  test("switch workspace in A while B's approve is still on its way", async ({ persona, page, api }) => {
    const { ledger, syncForeign } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    await proposeIn(b)
    ledger.confirmDelay = 1500
    allowAborts(b.page)
    await approve(b.page)
    await switchWorkspace(page, RUAHA.display_name)
    await expect.poll(() => savedWorkspace(b.page)).toBe("w2")
    await syncForeign()
    await b.page.waitForTimeout(2000) // the w1 answer lands after the switch
    console.log("[probe] B main after late answer:", (await b.page.locator("main").innerText()).slice(0, 300))
    await expect(b.page.locator("[data-receipt]")).toHaveCount(0)
    await persona.check()
  })

  test("log out in A and straight back in while B's approve is still on its way", async ({ persona, page, api }) => {
    const { ledger, syncForeign } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    await proposeIn(b)
    ledger.confirmDelay = 3000
    allowAborts(b.page)
    await approve(b.page)
    await logOut(page)
    await expect(page).toHaveURL(LOGIN)
    await page.getByLabel("Email").fill("e2e@genesy.test")
    await page.getByLabel("Password").fill("e2e-password-only")
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(page).not.toHaveURL(LOGIN) // back in (to `next` when it was given)
    await b.page.goto("/").catch(() => {})
    await b.page.waitForTimeout(3500)
    await syncForeign()
    console.log("[probe] B after relogin:", b.page.url(), (await b.page.locator("main").innerText().catch(() => "")).slice(0, 300))
    await expect(b.page.locator("[data-receipt]")).toHaveCount(0)
    await persona.check()
  })

  test("B approves in the same moment A switches workspace", async ({ persona, page, api }) => {
    const { ledger, syncForeign } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    await proposeIn(b)
    await Promise.all([switchWorkspace(page, RUAHA.display_name), approve(b.page)])
    await expect.poll(() => savedWorkspace(b.page)).toBe("w2")
    await b.page.waitForTimeout(1500)
    await syncForeign()
    console.log("[probe] decisions", ledger.decisions, "B:", (await b.page.locator("main").innerText()).slice(0, 300))
    await persona.check()
  })

  test("both tabs talk at once, then one reloads: each keeps its own conversation", async ({ persona, page, api }) => {
    const { ledger, tabs } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    for (const t of [tabs[0]!, b]) await expect.poll(async () => (await live(t.page)) || ((await startText(t.page)) && false), { timeout: 10_000 }).toBe(true)
    await Promise.all([say(page, "alpha from A"), say(b.page, "beta from B")])
    await expect(page.getByRole("log")).toContainText("Noted: alpha from A")
    await expect(b.page.getByRole("log")).toContainText("Noted: beta from B")
    await expect(page.getByRole("log")).not.toContainText("beta from B")
    await expect(b.page.getByRole("log")).not.toContainText("alpha from A")
    allowAborts(b.page)
    await b.page.reload()
    await expect(page.getByRole("log")).toContainText("Noted: alpha from A")
    await say(page, "still A")
    await expect(page.getByRole("log")).toContainText("Noted: still A")
    await persona.check()
  })
})

test.describe("findings 2", () => {
  // P5-2: B approves; before Office answers, A switches workspace (shared login → B follows).
  // The write ran (Office: completed), but B never says so: not in w2 (right), and not after
  // switching back to w1 either (the outcome is dropped with the remounted provider, the task
  // memory is reset on the owner change, and the recovered "completed" task is never shown).
  test("P5-2 an approve still on its way when another tab switches workspace: its outcome is reachable back in that workspace", async ({ persona, page, api }) => {
    const { ledger } = await setup(persona, page, api)
    const b = await openTab(persona, api, ledger, 1)
    await proposeIn(b)
    ledger.confirmDelay = 1500
    allowAborts(b.page)
    await approve(b.page)
    await switchWorkspace(page, RUAHA.display_name)
    await expect.poll(() => savedWorkspace(b.page)).toBe("w2")
    await expect.poll(() => [...ledger.reviews.values()][0]?.decided).toBe("approve")
    await switchWorkspace(page, ILUNDO.display_name)
    await expect.poll(() => savedWorkspace(b.page)).toBe("w1")
    await expect.poll(() => ledger.restores.includes("1:status")).toBe(true)
    // The approved change's result: its receipt, or at least a line naming it.
    await expect(b.page.locator("[data-receipt]").or(b.page.getByText(/Set stock levels/)).first()).toBeVisible({ timeout: 5_000 })
  })
})
