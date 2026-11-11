// Persona 8, the idle returner: leaves mid-flow (an open review, a half-typed draft, an open scene,
// files in the tray, a receipt) and comes back after the login expired, the review expired, the tab
// sat hidden, the laptop slept, or another tab logged out. Expects a clean resume, never a duplicate
// write, a stuck spinner, a raw error or another user's data. SIMULATED API and voice (harness.ts).
import type { Page } from "@playwright/test"

import { sc } from "../scenes-api"
import { k, readBack, scene, stockRoutes } from "../task"
import { v } from "../voice"
import { expect, test, type Persona } from "./harness"
import { batchRoutes, hide, logInAgain, photo, Server, sleep, tray } from "./idle-returner.helpers"
import { LOGIN, logOut, onLogin } from "./multitasker.helpers"

const MIN = 60_000
const ENDED = "Your session ended. Log in again to carry on."
const DRAFT = "Half a thought about honey"
const live = (page: Page) => page.getByRole("button", { name: v.addFiles }).isVisible().catch(() => false)

async function setup(persona: Persona, page: Page, api: import("../../office/fake-api").FakeApi, { clock = true } = {}) {
  const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
  const confirms = await stockRoutes(page)
  const server = new Server(api)
  await server.attach(page)
  if (clock) await page.clock.install()
  await page.goto("/")
  return { server, confirms }
}
async function talk(page: Page) {
  await expect
    .poll(async () => (await live(page)) || ((await page.getByRole("button", { name: v.textInstead, exact: true }).click({ timeout: 1_000 }).catch(() => {})) && false), { timeout: 10_000 })
    .toBe(true)
}
async function propose(persona: Persona, page: Page, total = 120) {
  await talk(page)
  persona.agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: total }] },
  })
  await expect(scene(page).getByRole("button", { name: v.approve })).toBeVisible()
}
test.describe("comes back cleanly", () => {
  test("tab hidden with a review open: cancelled once, said plainly, nothing to approve", async ({ persona, page, api }) => {
    const { confirms } = await setup(persona, page, api)
    await propose(persona, page)
    await hide(page, true)
    await hide(page, false)
    await expect(page.getByText(v.hiddenCancelled)).toBeVisible()
    await expect(page.getByRole("button", { name: v.approve })).toHaveCount(0)
    expect(confirms.map((c) => c.decision)).toEqual(["cancel"])
    await persona.check()
  })

  test("screen on past the review's expiry: explained, nothing sent, nothing to approve", async ({ persona, page, api }) => {
    const { confirms } = await setup(persona, page, api)
    await propose(persona, page)
    await sleep(page, 11 * MIN, { hidden: false })
    await expect(page.getByText(v.expired).first()).toBeVisible()
    await expect(page.getByRole("button", { name: v.approve })).toHaveCount(0)
    expect(confirms).toEqual([])
    await persona.check()
  })

  test("conversation live, laptop sleeps 2 h, login expired: session ended, log in, clean home", async ({ persona, page, api }) => {
    const { server } = await setup(persona, page, api)
    await talk(page)
    server.expire(page)
    await sleep(page, 120 * MIN)
    await expect(page).toHaveURL(LOGIN)
    await expect(page.getByText(ENDED)).toBeVisible()
    await logInAgain(page)
    await expect(page).not.toHaveURL(LOGIN) // back in (to `next` when it was given)
    await talk(page)
    await persona.check()
  })

  test("another tab logs out while this one is hidden with a review: login, nothing kept, clean after", async ({ persona, page, api }) => {
    const { confirms } = await setup(persona, page, api)
    await propose(persona, page)
    await hide(page, true)
    const other = await page.context().newPage()
    await other.route("**/v1/**", (route) => api.handle(route))
    await other.goto("/")
    await logOut(other)
    await expect(other).toHaveURL(LOGIN)
    await hide(page, false)
    await expect(page).toHaveURL(LOGIN)
    await expect(page.getByText("Raw Honey 500g")).toHaveCount(0)
    await logInAgain(page)
    await expect(page).not.toHaveURL(LOGIN) // back in (to `next` when it was given)
    expect(confirms.filter((c) => c.decision === "approve")).toEqual([])
    await persona.check()
  })

  test("files in the tray, login expires: session ended, and nothing of the file after logging in", async ({ persona, page, api }) => {
    const { server } = await setup(persona, page, api)
    await talk(page)
    await page.getByLabel(v.chooseFiles).setInputFiles(photo)
    await expect(tray(page)).toContainText("shelf.png")
    server.expire(page)
    await sleep(page, 90 * MIN)
    await expect(page).toHaveURL(LOGIN)
    await expect(page.getByText(ENDED)).toBeVisible()
    await logInAgain(page)
    await expect(page).not.toHaveURL(LOGIN) // back in (to `next` when it was given)
    await expect(page.getByText("shelf.png")).toHaveCount(0)
    await persona.check()
  })

  test("receipt shown, away an hour: the receipt is still there", async ({ persona, page, api }) => {
    await setup(persona, page, api)
    await propose(persona, page)
    await scene(page).getByRole("button", { name: v.approve }).click()
    await expect(page.locator("[data-receipt]")).toBeVisible()
    await sleep(page, 60 * MIN)
    await expect(page.locator("[data-receipt]")).toBeVisible()
    await persona.check()
  })

  test("scene open, away 30 min: marked outdated (or refreshed), never stuck", async ({ persona, page, api }) => {
    await setup(persona, page, api)
    await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click()
    await expect(page).toHaveURL(/\/scenes\//)
    await sleep(page, 30 * MIN)
    await expect(page.getByText(/Data may be outdated|Updated \d+ s ago/).first()).toBeVisible()
    await persona.check()
  })
})

test.describe("findings", () => {
  // P8-1 (fixed, regression guard): a half-typed message is lost without a word when the login expires and the person logs
  // in again: the box is empty (no conversation), nothing says the draft was not sent.
  test("P8-1 a half-typed draft survives an expired login, or the person is told it was lost", async ({ persona, page, api }) => {
    const { server } = await setup(persona, page, api)
    await talk(page)
    await page.getByLabel(v.message).fill(DRAFT)
    server.expire(page)
    await sleep(page, 90 * MIN)
    await expect(page).toHaveURL(LOGIN)
    await logInAgain(page)
    await expect(page).not.toHaveURL(LOGIN) // back in (to `next` when it was given)
    const kept = page.getByLabel(v.message).and(page.locator(`[value="${DRAFT}"]`))
    await expect(kept.or(page.getByText(DRAFT)).or(page.getByText(/draft/i)).first()).toBeVisible({ timeout: 5_000 })
  })

  // P8-2 (fixed, regression guard): a review that simply expired turns the orb to its error state: "Something went wrong"
  // next to "This review expired." (orb.ts: any voice.error is "error").
  test("P8-2 an expired review is not announced as \"Something went wrong\"", async ({ persona, page, api }) => {
    await setup(persona, page, api)
    await propose(persona, page)
    await sleep(page, 11 * MIN, { hidden: false })
    await expect(page.getByText(v.expired).first()).toBeVisible()
    await expect(page.getByText("Something went wrong", { exact: true })).toHaveCount(0)
  })

  // P8-3 (fixed, regression guard): the login expired on a scene; after logging in again the person lands on home, not on
  // the scene they were on (the login page always goes to "/").
  test("P8-3 after an expired login, logging in again goes back to the scene the person was on", async ({ persona, page, api }) => {
    const { server } = await setup(persona, page, api)
    await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click()
    await expect(page).toHaveURL(/\/scenes\//)
    const where = new URL(page.url()).pathname
    server.expire(page)
    await sleep(page, 30 * MIN)
    await page.getByRole("button", { name: "Refresh" }).first().click({ timeout: 3_000 }).catch(() => {})
    await expect(page).toHaveURL(LOGIN)
    await expect(page.getByText(ENDED)).toBeVisible()
    await logInAgain(page)
    await expect(page).toHaveURL(new RegExp(`${where}$`))
  })

  // P8-4: Approve pressed after the login expired: the confirm is refused (401, nothing ran), the
  // person logs in again and the review is simply gone. Nothing says it did not run or offers it
  // again, so they cannot tell whether the change happened.
  test.fail("P8-4 an approve refused by an expired login is reachable (or said not to have run) after logging in", async ({ persona, page, api }) => {
    const { confirms, server } = await setup(persona, page, api)
    await propose(persona, page)
    server.expire(page)
    await scene(page).getByRole("button", { name: v.approve }).click()
    await expect(page).toHaveURL(LOGIN)
    expect(confirms).toEqual([]) // refused before Office: nothing ran
    await logInAgain(page)
    await expect(page).not.toHaveURL(LOGIN) // back in (to `next` when it was given)
    const again = page.getByRole("button", { name: v.approve }).or(page.getByRole("button", { name: "Continue review" }))
    await expect(again.or(page.getByText(/not (run|approved|sent)|did not/i)).first()).toBeVisible({ timeout: 5_000 })
  })
})

test("idle returner: works, leaves mid-flow, comes back after expiry, sleep, hidden tab or a logout elsewhere", async ({ persona, page, api }) => {
  const { server } = await setup(persona, page, api)
  await page.emulateMedia({ colorScheme: persona.chance(0.5) ? "dark" : "light" })
  let other: Page | null = null
  /** Back at the desk: if the login page shows, it says why (when the login expired) and they log in. */
  const comeBack = async (expired: boolean) => {
    await page.waitForLoadState().catch(() => {})
    if (!onLogin(page)) {
      // A stale page is acted on: the first call learns the login is gone.
      if (expired) await page.getByRole("button", { name: "Refresh" }).first().click({ timeout: 1_000 }).catch(() => {})
      if (expired) await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click({ timeout: 1_000 }).catch(() => {})
      if (expired) await talk(page).catch(() => {})
    }
    if (expired) await expect(page).toHaveURL(LOGIN)
    if (!onLogin(page)) return
    if (expired) await expect(page.getByText(ENDED)).toBeVisible()
    await logInAgain(page)
    await expect(page).not.toHaveURL(LOGIN) // back in (to `next` when it was given)
  }
  const away = () => persona.pick([1, 5, 11, 30, 90, 180]) * MIN

  await persona.run(
    [
      { name: "start typing", weight: 2, run: () => persona.startText() },
      { name: "ask", weight: 3, run: () => persona.say(persona.pick(["Sales today?", "Which orders wait?", "Stock below minimum"])) },
      {
        name: "a stock change to review",
        weight: 3,
        run: async () => {
          if (!(await live(page))) return
          persona.agent.call("stock_set_levels", {
            path: { product: "p-500" },
            body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: persona.pick([110, 120, 130]) }] },
          })
        },
      },
      { name: "approve", weight: 2, run: () => persona.click(scene(page).getByRole("button", { name: v.approve })) },
      {
        name: "half-type a message",
        weight: 2,
        run: () => page.getByLabel(v.message).fill(DRAFT, { timeout: 1_000 }).catch(() => {}),
      },
      {
        name: "files in the tray",
        weight: 1,
        run: async () => {
          if (await live(page)) await page.getByLabel(v.chooseFiles).setInputFiles(photo, { timeout: 1_000 }).catch(() => {})
        },
      },
      {
        name: "open a scene",
        weight: 2,
        run: async () => {
          await persona.backHome()
          const links = page.getByRole("region", { name: sc.yourFlow }).getByRole("link")
          const n = await links.count()
          if (n) await persona.click(links.nth(persona.int(0, n - 1)))
        },
      },
      { name: "back home", weight: 1, run: () => persona.backHome() },
      { name: "reload", weight: 1, run: () => persona.reload() },
      {
        name: "tab hidden a while, back",
        weight: 2,
        run: async () => {
          await hide(page, true)
          await page.clock.fastForward(away())
          await hide(page, false)
          await comeBack(false)
        },
      },
      { name: "laptop sleeps (screen on)", weight: 2, run: async () => (await sleep(page, away(), { hidden: false }), comeBack(false)) },
      { name: "laptop sleeps (lid closed)", weight: 2, run: async () => (await sleep(page, away()), comeBack(false)) },
      {
        name: "login expires while away",
        weight: 2,
        run: async () => {
          server.expire(page)
          await sleep(page, away() + 60 * MIN, { hidden: persona.chance(0.5) })
          await comeBack(true)
        },
      },
      {
        name: "another tab logs out while this one is hidden",
        weight: 1,
        run: async () => {
          await hide(page, true)
          other ??= await page.context().newPage()
          await other.route("**/v1/**", (route) => api.handle(route)).catch(() => {})
          await other.goto("/")
          await logOut(other).catch(() => {})
          await expect(other).toHaveURL(LOGIN)
          await hide(page, false)
          await expect(page).toHaveURL(LOGIN)
          await comeBack(false)
        },
      },
    ],
    { count: Number(process.env.PERSONA_STEPS) || 25 }
  )
})


test.describe("a running batch", () => {
  async function run(persona: Persona, page: Page, api: import("../../office/fake-api").FakeApi) {
    const { server } = await setup(persona, page, api)
    const batch = await batchRoutes(page) // after setup: its confirm answers before stockRoutes'
    await server.attach(page) // and the login check answers before everything
    await talk(page)
    persona.agent.call("assistant_batch_prepare", { calls: [{}, {}] })
    await expect(page.locator("[data-batch]")).toContainText("21 changes matched")
    await scene(page).getByRole("button", { name: v.approve }).click()
    await expect.poll(() => batch.confirms).toEqual(["approve"])
    return { batch, server }
  }

  test("tab hidden while it runs: paused, said so, nothing runs twice", async ({ persona, page, api }) => {
    const { batch } = await run(persona, page, api)
    await hide(page, true)
    await page.clock.fastForward(30 * MIN)
    await hide(page, false)
    await expect(page.locator("[data-batch]")).toHaveAttribute("data-batch", "paused")
    await expect(page.locator("[data-batch]")).toContainText("8 updated · 0 failed · 13 not run")
    expect(batch.commands.filter((c) => c !== "status")).toEqual(["prepare", "pause"])
    expect(batch.confirms).toEqual(["approve"])
    await persona.check()
  })

  // P8-5 (fixed, regression guard): a paused run is labelled "Genesy is working" in the task line (capsule.tsx: only ENDED
  // states get their own label; "paused" falls back to "working").
  test("P8-5 a paused run is not labelled \"Genesy is working\"", async ({ persona, page, api }) => {
    await run(persona, page, api)
    await hide(page, true)
    await hide(page, false)
    await expect(page.locator("[data-batch]")).toHaveAttribute("data-batch", "paused")
    const line = page.getByRole("status", { name: k.label })
    await expect(line).toHaveAttribute("data-task-state", "paused")
    await expect(line).not.toContainText(k.working)
  })

  // P8-6: the login expires while a run is going; after logging in again the run (still going, or
  // paused, at Office) is nowhere: no progress, no line about it, so the person cannot tell what ran.
  test.fail("P8-6 a run going when the login expired is reachable after logging in again", async ({ persona, page, api }) => {
    const { batch, server } = await run(persona, page, api)
    server.expire(page)
    await sleep(page, 30 * MIN, { hidden: false })
    await expect(page).toHaveURL(LOGIN)
    await expect(page.getByText(ENDED)).toBeVisible()
    await logInAgain(page)
    await expect(page).not.toHaveURL(LOGIN) // back in (to `next` when it was given)
    expect(batch.confirms).toEqual(["approve"])
    await expect(page.locator("[data-batch]").or(page.getByText(/supplier purchase prices/)).first()).toBeVisible({ timeout: 5_000 })
  })
})
