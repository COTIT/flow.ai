// Persona 3, the confused novice: clicks the wrong things, opens cards and abandons them, asks
// vague, misspelled or off-topic questions, cancels halfway and retries another way, dismisses
// hints then looks for them again, uses Back to "undo", leaves a review open and wanders off,
// tries what their role may not do, follows suggestions in random order.
// SIMULATED API and voice (see harness.ts).
import type { Page } from "@playwright/test"

import discoveryEn from "../../../../../apps/flow/messages/en/discovery.json" with { type: "json" }
import { member } from "../storefront-api"
import { menuItem, moreMenu, sc } from "../scenes-api"
import { s as shell } from "../shell"
import {
  capsule,
  k,
  proposeStock,
  readBack,
  receipt,
  scene,
  stockRoutes,
} from "../task"
import { startText, v } from "../voice"
import { markInjected } from "../../../src/test"
import { expect, test, type Persona, type Step } from "./harness"

/** Steps per walk: PERSONA_STEPS wins; CI runs a short walk (suite budget), local runs the full one. */
const STEPS = Number(process.env.PERSONA_STEPS) || (process.env.CI ? 12 : 40)

const d = discoveryEn.discovery

/** Things a confused person types: vague, misspelled, off-topic, empty, odd characters. */
const MUDDLE = [
  "help",
  "wat can u do",
  "stok hony?",
  "how much money did i make",
  "the thing from before",
  "undo",
  "cancel that",
  "no wait",
  "weather tomorrow?",
  "who are you",
  "ordrs",
  "   ",
  "??",
  "🙂",
  "<b>bold</b> & {curly}",
  "SELECT * FROM products;",
  "hey genesy",
  "a".repeat(600),
]

/** Raw codes or broken text a person must never see. */
const GARBLED =
  /\bundefined\b|\[object Object\]|\bNaN\b|\{(count|name|seconds|record|product|areas|summary)\}|\b(flow|shell|scenes|task|discovery|studio|voice)\.[a-z]+\.[a-z]/

/** Novice invariants on top of the harness's: a way back from anywhere, nothing garbled. */
async function expectNoDeadEnd(page: Page) {
  if (page.isClosed()) return
  const path = new URL(page.url()).pathname
  const home = page.locator('a[href="/"]:visible')
  const back = page.getByRole("button", { name: shell.backHome })
  // Polled: a step may end mid-navigation, before the next page drew its way back.
  if (path !== "/")
    await expect
      .poll(async () => (await home.count()) + (await back.count()), {
        message: `dead end at ${path}: no way back home`,
        timeout: 3_000,
      })
      .toBeGreaterThan(0)
  const text = await page
    .locator("body")
    .innerText()
    .catch(() => "")
  expect(text.match(GARBLED)?.[0], `garbled text at ${path}`).toBeUndefined()
}

/** Types only if the box can take it (a review open makes the composer inert, as it should). */
async function mutter(persona: Persona, page: Page, text: string) {
  const box = page.getByLabel(v.message)
  if (!(await box.isVisible().catch(() => false))) return false
  if (await inert(page)) return false
  return persona.say(text)
}

const inert = (page: Page) =>
  page
    .getByLabel(v.message)
    .evaluate((b) => !!b.closest("[inert]"), undefined, { timeout: 1_000 })
    .catch(() => true)

const live = (page: Page) =>
  page
    .getByRole("button", { name: v.addFiles })
    .isVisible()
    .catch(() => false)

function noviceSteps(persona: Persona, page: Page): Step[] {
  const yourFlow = page.getByRole("region", { name: sc.yourFlow })
  const pills = page.getByRole("list", {
    name: /Suggested for you|Here's what you can do next/,
  })
  return [
    {
      name: "click something, anything",
      weight: 5,
      run: async () =>
        persona.log(
          `   clicked: ${await persona.clickAnything(page.locator("main"))}`
        ),
    },
    {
      name: "open a scene, then abandon it",
      weight: 3,
      run: async () => {
        await persona.backHome()
        const links = yourFlow.getByRole("link")
        const n = await links.count()
        if (!n) return
        await persona.click(links.nth(persona.int(0, n - 1)))
        if (persona.chance(0.5)) await persona.history("back")
        else await persona.backHome()
      },
    },
    {
      name: "type something vague or misspelled",
      weight: 4,
      run: async () => {
        const text = persona.pick(MUDDLE)
        persona.log(`   typed: ${JSON.stringify(text.slice(0, 30))}`)
        await mutter(persona, page, text)
      },
    },
    {
      name: "start typing a conversation",
      weight: 2,
      run: () => persona.startText(),
    },
    {
      name: "Genesy proposes a stock change",
      weight: 2,
      run: async () => {
        if (!(await live(page))) return
        persona.agent.call("stock_set_levels", {
          path: { product: "p-500" },
          body: {
            warehouses: [
              {
                warehouse_id: "wh-main",
                stock_lvl: persona.pick([110, 120, 130]),
              },
            ],
          },
        })
      },
    },
    {
      name: "Genesy asks which product",
      weight: 1,
      run: async () => {
        if (!(await live(page))) return
        persona.agent.call("products_list", {})
        await page.waitForTimeout(150)
        persona.agent.say("Which product did you mean?")
      },
    },
    {
      name: "pick a product in the choice, or wander off",
      weight: 2,
      run: async () => {
        const choice = scene(page).getByRole("radio")
        const n = await choice.count()
        if (!n) return
        await persona.click(choice.nth(persona.int(0, n - 1)))
        if (persona.chance(0.5)) await persona.backHome()
        else
          await persona.click(
            scene(page).getByRole("button", { name: k.clarify.continue })
          )
      },
    },
    {
      name: "cancel halfway",
      weight: 2,
      run: () =>
        persona.click(
          scene(page).getByRole("button", { name: v.cancel, exact: true })
        ),
    },
    {
      name: "approve, eventually",
      weight: 1,
      run: () =>
        persona.click(scene(page).getByRole("button", { name: v.approve })),
    },
    {
      name: "open the review from the task line, then leave it open",
      weight: 2,
      run: async () => {
        await persona.click(
          capsule(page).getByRole("button", { name: k.review })
        )
        await persona.backHome()
        const links = yourFlow.getByRole("link")
        const n = await links.count()
        if (n) await persona.click(links.nth(persona.int(0, n - 1)))
      },
    },
    {
      name: "dismiss whatever is dismissable",
      weight: 1,
      run: async () =>
        persona.click(
          page.getByRole("button", {
            name: persona.pick([d.hint.dismiss, k.dismiss, k.resume.dismiss]),
          })
        ),
    },
    {
      name: "look for the tip again",
      weight: 1,
      run: async () => {
        await persona.backHome()
        if (
          await persona.click(page.getByRole("link", { name: d.hint.explore }))
        )
          return
        await persona.click(
          pills.getByRole("button", { name: d.capabilities.explore.label })
        )
      },
    },
    {
      name: "in the explorer: wander tabs, try something",
      weight: 2,
      run: async () => {
        if (new URL(page.url()).pathname !== "/explore") return
        const tabs = page.getByRole("tab")
        const n = await tabs.count()
        if (n) await persona.click(tabs.nth(persona.int(0, n - 1)))
        const tries = page.getByRole("button", { name: /^Try / })
        const m = await tries.count()
        if (m && persona.chance(0.6))
          await persona.click(tries.nth(persona.int(0, m - 1)))
        else if (persona.chance(0.3))
          await persona.click(
            page.getByRole("button", { name: d.explorer.reset })
          )
      },
    },
    {
      name: "follow a random suggestion",
      weight: 4,
      run: async () => {
        const buttons = pills.getByRole("button")
        const n = await buttons.count()
        if (!n) return
        const pill = buttons.nth(persona.int(0, n - 1))
        persona.log(
          `   pill: ${await pill.innerText({ timeout: 1_000 }).catch(() => "?")}`
        )
        await persona.click(pill)
      },
    },
    { name: "Back to undo", weight: 3, run: () => persona.history("back") },
    { name: "Forward again", weight: 1, run: () => persona.history("forward") },
    { name: "escape", weight: 1, run: () => persona.keys("Escape") },
    {
      name: "send an empty box",
      weight: 1,
      run: async () => {
        if (await inert(page)) return
        const box = page.getByLabel(v.message)
        await box.fill("", { timeout: 1_000 }).catch(() => {})
        await box.press("Enter", { timeout: 1_000 }).catch(() => {})
      },
    },
    { name: "reload in panic", weight: 1, run: () => persona.reload() },
    {
      name: "storefront",
      weight: 1,
      run: () =>
        persona.click(
          page.getByRole("link", { name: "Storefront", exact: true })
        ),
    },
    {
      name: "Your Flow: reset, then change mind",
      weight: 1,
      run: async () => {
        await persona.backHome()
        if (
          !(await persona.click(
            yourFlow.getByRole("button", { name: sc.reset })
          ))
        )
          return
        const dialog = page.getByRole("alertdialog")
        await persona.click(
          dialog.getByRole("button", {
            name: persona.chance(0.7) ? sc.cancel : sc.reset,
          })
        )
      },
    },
    {
      name: "remove a scene by mistake, then restore it",
      weight: 1,
      run: async () => {
        // Remove lives in the scene's More menu.
        if (!(await persona.click(moreMenu(page)))) return
        if (!(await persona.click(menuItem(page, sc.remove)))) return
        await persona.backHome()
        await persona.click(
          yourFlow.getByRole("button", { name: new RegExp(`^${sc.restore} `) })
        )
      },
    },
    {
      name: "end the conversation",
      weight: 1,
      run: () => persona.click(page.getByRole("button", { name: v.end })),
    },
    {
      name: "a request fails underneath",
      weight: 1,
      run: async () => {
        const path = persona.pick([
          /\/v1\/workspace\/w1\/flow\/scenes/,
          /\/v1\/workspace\/w1\/attention/,
          /\/api\/voice\/tools/,
        ])
        persona.log(`   failing: ${path}`)
        await persona.failApi(path, persona.pick([500, 503, 429]))
        await persona.reload()
      },
    },
    {
      name: "wifi drops while clicking, then returns",
      weight: 1,
      run: async () => {
        await persona.offline(page.context(), true)
        // Buttons only: a link tapped offline is P3-4 (its own repro below).
        const buttons = page.locator("main button:visible:not([disabled])")
        const n = await buttons.count()
        if (n) await persona.click(buttons.nth(persona.int(0, n - 1)))
        await page.waitForTimeout(500)
        await persona.offline(page.context(), false)
      },
    },
    {
      name: "tab around",
      weight: 1,
      run: () => persona.tab(persona.int(1, 5), persona.chance(0.3)),
    },
  ].map((step) => ({
    ...step,
    run: async () => {
      await step.run()
      await expectNoDeadEnd(page)
    },
  }))
}

/** PERSONA_TRAIL=1 prints the walk, to see which steps really landed. */
async function walk(persona: Persona, page: Page) {
  try {
    await persona.run(noviceSteps(persona, page), { count: STEPS })
  } finally {
    if (process.env.PERSONA_VERBOSE) console.log(persona.trail.join("\n"))
  }
}

async function setup(
  page: Page,
  api: Parameters<Parameters<typeof test>[2]>[0]["api"]
) {
  const stock =
    readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on(
    "GET",
    "/workspace/:ws/products/:id/stock-with-bins",
    () => stock.body as object
  )
  await stockRoutes(page)
}

test("confused novice: wrong clicks, abandoned cards, vague words, Back to undo", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await page.goto("/")
  await walk(persona, page)
})

test.describe("in the dark", () => {
  test.use({ colorScheme: "dark" })
  test("confused novice at night: same muddle, dark theme", async ({
    persona,
    page,
    api,
  }) => {
    await setup(page, api)
    await page.goto("/")
    await walk(persona, page)
  })
})

test("confused novice without rights: a plain member wanders everywhere", async ({
  persona,
  page,
  api,
}) => {
  api.on("GET", "/me", () => ({ data: { user: member } }))
  await setup(page, api)
  await page.goto("/")
  await walk(persona, page)
})

// ---- Targeted probes: the novice's stories, played out exactly. ----------------------------

// P3-1: the explorer is reachable only from the first-visit tip. Once it is dismissed (or after 5
// uses) an owner's 4 home suggestions are full (briefing, alert, review alerts, pending orders),
// so "Explore possibilities" (last in the registry) never shows: no way back to it.
test("P3-1: after dismissing the tip, the explorer can still be found from home", async ({
  persona,
  page,
  api,
}) => {
  void persona
  await setup(page, api)
  await page.goto("/")
  await page.getByRole("button", { name: d.hint.dismiss }).click()
  await expect(
    page.getByRole("complementary", { name: d.hint.lead })
  ).toHaveCount(0)
  await expect(page.getByRole("region", { name: sc.yourFlow })).toBeVisible()
  // The novice looks for it again: a link to the explorer, or its suggestion.
  const way = page
    .locator('a[href="/explore"]:visible')
    .or(page.getByRole("button", { name: d.capabilities.explore.label }))
  await expect(way.first()).toBeVisible()
})

// P3-2: "Try it" sends the instruction and Genesy answers, but the person stays on /explore,
// where the conversation is not shown: nothing on screen says what happened.
test("P3-2: Try it in the explorer shows what Genesy answered", async ({
  persona,
  page,
  api,
}) => {
  void persona
  await setup(page, api)
  await page.goto("/")
  await page.getByRole("link", { name: d.hint.explore }).click()
  await expect(page).toHaveURL(/\/explore$/)
  await page.getByRole("tab", { name: d.explorer.groups.understand }).click()
  await page
    .getByRole("button", { name: `Try ${d.capabilities.weekly_sales.label}` })
    .click()
  // It was sent and answered...
  const prompt = d.capabilities.weekly_sales.prompt
  await expect
    .poll(() =>
      persona.agent.sent.some(
        (e) => e.type === "user_message" && e.text === prompt
      )
    )
    .toBe(true)
  await expect(page).toHaveURL(/\/explore$/)
  // ...so the person, still here, must see it (or be taken to the conversation).
  // The fake agent answers each typed line with "Noted: <line>".
  await expect(
    page.getByText(`Noted: ${d.capabilities.weekly_sales.prompt}`).first()
  ).toBeVisible()
})

/** While talking, home is the conversation; its own Back to home leads to Your Flow. */
async function toYourFlow(page: Page) {
  const flow = page.getByRole("region", { name: sc.yourFlow })
  for (let i = 0; i < 3 && !(await flow.isVisible()); i++)
    await page.getByRole("button", { name: shell.backHome }).click()
  await expect(flow).toBeVisible()
}

test("probe: a review left open survives wandering through scenes and Back", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await proposeStock(page, persona.agent, 120)
  // Wanders off: home, a scene, Back, another scene; the task line follows.
  await toYourFlow(page)
  await page
    .getByRole("region", { name: sc.yourFlow })
    .getByRole("link", { name: /Stock Watch/ })
    .click()
  await expect(capsule(page)).toHaveAttribute(
    "data-task-state",
    "needs_approval"
  )
  await page.goBack()
  await toYourFlow(page)
  await page
    .getByRole("region", { name: sc.yourFlow })
    .getByRole("link", { name: /Order Focus/ })
    .click()
  await expectNoDeadEnd(page)
  // Comes back through the task line: the same review, still decidable.
  await capsule(page).getByRole("button", { name: k.review }).click()
  await expect(scene(page)).toContainText("Confirm action 135790")
  await expect(
    scene(page).getByRole("button", { name: v.approve })
  ).toBeEnabled()
  await persona.check()
})

test("probe: cancel halfway, then the same goal another way", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  const stock =
    readBack("130.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on(
    "GET",
    "/workspace/:ws/products/:id/stock-with-bins",
    () => stock.body as object
  )
  await proposeStock(page, persona.agent, 120)
  await scene(page).getByRole("button", { name: v.cancel, exact: true }).click()
  await expect(
    scene(page).getByRole("button", { name: v.approve })
  ).toHaveCount(0)
  // Retries by typing, Genesy proposes again with another number.
  await page
    .getByRole("button", { name: shell.backHome })
    .click()
    .catch(() => {})
  await persona.say("set the hony stock to 130 pls", "enter")
  persona.agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 130 }] },
  })
  await expect(page.locator("[data-withdrawn]")).toContainText("→ 120")
  await expect(capsule(page)).toHaveAttribute(
    "data-task-state",
    "needs_approval"
  )
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("130")
  expect(persona.confirms).toEqual([
    { ticket: "stock-120", decision: "cancel" },
    { ticket: "stock-130", decision: "approve" },
  ])
  await persona.check()
})

test("probe: Back after approving does not bring the approval back", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await proposeStock(page, persona.agent, 120)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
  for (const step of ["back", "forward", "back", "back", "forward"] as const) {
    await persona.history(step)
    await expect(page.getByRole("button", { name: v.approve })).toHaveCount(0)
    await expectNoDeadEnd(page)
  }
  expect(persona.confirms.filter((c) => c.decision === "approve")).toHaveLength(
    1
  )
  await persona.check()
})

test("probe: a member asks for a change they may not make and is told so", async ({
  persona,
  page,
  api,
}) => {
  api.on("GET", "/me", () => ({ data: { user: member } }))
  await page.route("**/api/voice/tools", async (route) => {
    const b = (route.request().postDataJSON() ?? {}) as {
      command?: string
      operationId?: string
    }
    if (b.command === "execute" && b.operationId === "stock_set_levels") {
      markInjected(page, /\/api\/voice\/tools/)
      return route.fulfill({ status: 403, json: { message: "Forbidden" } })
    }
    return route.fallback()
  })
  await page.goto("/")
  await startText(page)
  await persona.say("make honey 120", "enter")
  persona.agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })
  // Plain words, no code; and the person can carry on typing.
  await expect(page.getByText(v.requestErrors.forbidden).first()).toBeVisible()
  await expect(page.getByText(/\b403\b|Forbidden/)).toHaveCount(0)
  await expect(page.getByLabel(v.message)).toBeEditable()
  expect(await inert(page)).toBe(false)
  await expectNoDeadEnd(page)
  await persona.check()
})

// P3-3: the stock review leads with only "Product: Raw Honey 500g" under the title "Run action for
// stock"; the amount being set (14 → 130) is not on the card. A novice approves without seeing
// the number unless they open "More details". Fixed by P3-3: the API's stock facts carry
// `changes` rows (warehouse · before → after) that lead the card; kept as a regression guard.
test("P3-3: a stock review shows the new amount before approving, without opening details", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await proposeStock(page, persona.agent, 130)
  const card = scene(page)
  await expect(card.getByRole("button", { name: v.approve })).toBeVisible()
  await expect(card).toContainText("130")
})

// P3-4: offline, a scene link's RSC fetch fails and Next falls back to a full browser navigation:
// the person lands on Chrome's offline error page (chrome-error://chromewebdata/), Flow is gone,
// and it stays gone when the network returns.
test("P3-4: a scene link tapped while the wifi is down keeps the person in Flow", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await page.goto("/")
  const link = page
    .getByRole("region", { name: sc.yourFlow })
    .getByRole("link", { name: /Stock Watch/ })
  await expect(link).toBeVisible()
  await persona.offline(page.context(), true)
  await link.click()
  await page.waitForTimeout(1_500)
  expect(page.url(), "left Flow for the browser's offline page").not.toMatch(
    /^chrome-error:/
  )
  await expect(page.getByRole("region", { name: shell.dock })).toBeVisible()
  await persona.offline(page.context(), false)
})
