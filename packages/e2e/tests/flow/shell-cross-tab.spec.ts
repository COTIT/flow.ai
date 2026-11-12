import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { BARAKA, flowApi, ILUNDO, mockApi, RUAHA } from "./api"
import {
  designs,
  draft,
  otherDraft,
  previewFrame,
  revisionButton,
  root,
  test as storefrontTest,
} from "./storefront-api"
import { fakeAgent, send, startText, typeInstead } from "./voice"

// Two tabs = two pages of one browser context (one localStorage). Tab 2 writes the saved login
// exactly as @genesy/api-client's sessionStore does; tab 1 (the guarded page) must follow.
// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

const KEY = "genesy.session"
const saved = (
  token: string | null,
  workspaceIds: string[],
  workspaceId: string | null
) => JSON.stringify({ state: { token, workspaceIds, workspaceId }, version: 0 })

/** Tab 2 on a Flow page that runs no app code, with its own error checks. */
async function otherTab(page: Page) {
  const tab = await page.context().newPage()
  const problems: string[] = []
  tab.on("pageerror", (e) => problems.push(`page error: ${e.message}`))
  tab.on("console", (m) => {
    if (m.type() === "error") problems.push(`console error: ${m.text()}`)
  })
  await tab.goto("/healthz")
  const write = (steps: [string, string | null][]) =>
    tab.evaluate((list) => {
      for (const [key, value] of list)
        if (value === null) localStorage.removeItem(key)
        else localStorage.setItem(key, value)
    }, steps)
  // signOut(): set the logged-out state, then drop the entry.
  const logOut: [string, string | null][] = [
    [KEY, saved(null, [], null)],
    [KEY, null],
  ]
  return {
    problems,
    logOut: () => write(logOut),
    selectWorkspace: (ids: string[], id: string, token = "e2e-token") =>
      write([[KEY, saved(token, ids, id)]]),
    // Tab 2's logout then login, in one go.
    logOutThenIn: (token: string) =>
      write([...logOut, [KEY, saved(token, ["w1"], "w1")]]),
    // Another login replacing this one, as tab 1 sees it when it never saw the logout between.
    replaceLogin: (token: string) => write([[KEY, saved(token, ["w1"], "w1")]]),
    savedToken: () =>
      tab.evaluate(
        (key) =>
          (
            JSON.parse(localStorage.getItem(key) ?? "{}") as {
              state?: { token?: string | null }
            }
          ).state?.token ?? null,
        KEY
      ),
    writeOther: () => write([["genesy.other", "x"]]),
  }
}

/** Voice session ends tab 1 sends, with their answers. */
function voiceEnds(page: Page) {
  const ends: { status: number; body: unknown; auth?: string }[] = []
  page.on("response", (r) => {
    const req = r.request()
    if (new URL(r.url()).pathname !== "/api/voice/session") return
    if (req.method() !== "DELETE") return
    ends.push({
      status: r.status(),
      body: req.postDataJSON(),
      auth: req.headers().authorization,
    })
  })
  return ends
}

async function talk(page: Page, answer: string) {
  await fakeAgent(page, (text, a) => a.say(`${answer} ${text}`))
  await page.goto("/")
  await startText(page)
  await send(page, "private question")
  await expect(page.getByRole("log")).toContainText(answer)
}

test("logging out in another tab ends the voice session here and goes to the login page", async ({
  page,
}) => {
  await seedSession(page)
  await mockApi(page, flowApi())
  const ends = voiceEnds(page)
  let logouts = 0
  page.on("request", (r) => {
    if (new URL(r.url()).pathname === "/v1/logout") logouts++
  })
  await talk(page, "Private answer to")
  const tab = await otherTab(page)

  await tab.logOut()
  await expect(page).toHaveURL("/login")
  await expect.poll(() => ends.length).toBe(1)
  expect(ends[0]).toMatchObject({ status: 200, body: { workspaceId: "w1" } })
  await expect(page.getByText("Private answer")).toHaveCount(0)
  // The other tab logged out; this one only forgets the login, it did not expire.
  await expect(page.getByText("Your session ended")).toHaveCount(0)
  expect(logouts).toBe(0)
  await expect(page.getByLabel("Email")).toBeVisible()
  await expectNoHorizontalScroll(page)
  expect(tab.problems).toEqual([])
})

test("switching workspace in another tab ends the voice session and shows nothing of the old one", async ({
  page,
}) => {
  await seedSession(page, { workspaceIds: ["w1", "w2"] })
  await mockApi(
    page,
    flowApi({ "GET /me/workspaces": { body: { data: [ILUNDO, RUAHA] } } })
  )
  const ends = voiceEnds(page)
  await talk(page, "Ilundo answer to")
  const tab = await otherTab(page)

  // Another key changing elsewhere is none of Flow's business.
  await tab.writeOther()
  await expect(page.getByRole("log")).toContainText("Ilundo answer to")
  expect(ends).toHaveLength(0)

  await tab.selectWorkspace(["w1", "w2"], "w2")
  await expect.poll(() => ends.length).toBe(1)
  expect(ends[0]).toMatchObject({
    status: 200,
    body: { workspaceId: "w1" },
    auth: "Bearer e2e-token",
  })
  await expect(
    page.getByRole("button", { name: typeInstead, exact: true })
  ).toBeVisible()
  await expect(page.getByText(RUAHA.display_name).first()).toBeVisible()
  await expect(page.getByText("Ilundo answer to")).toHaveCount(0)
  await expect(page.getByText("private question")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
  expect(tab.problems).toEqual([])
})

test("another user signing in elsewhere ends this conversation and shows only the new user", async ({
  page,
}) => {
  await seedSession(page)
  await mockApi(page, flowApi())
  const meCalls: string[] = []
  // Registered after mockApi, so it answers /me: who the token belongs to.
  await page.route("**/v1/me", (route) => {
    const auth = route.request().headers().authorization ?? ""
    meCalls.push(auth)
    return route.fulfill({
      json: {
        data: {
          user: auth.endsWith("-2")
            ? BARAKA
            : { ...BARAKA, id: "u1", name: "Amina Tester" },
        },
      },
    })
  })
  const ends = voiceEnds(page)
  await talk(page, "Amina answer to")
  await expect(page.getByText("Amina Tester")).toBeVisible()
  const tab = await otherTab(page)

  await tab.replaceLogin("e2e-token-2")
  await expect.poll(() => ends.length).toBe(1)
  expect(ends[0]).toMatchObject({
    status: 200,
    body: { workspaceId: "w1" },
    auth: "Bearer e2e-token",
  })
  await expect(page.getByText(BARAKA.name)).toBeVisible()
  await expect(page.getByText("Amina Tester")).toHaveCount(0)
  await expect(page.getByText("Amina answer to")).toHaveCount(0)
  expect(meCalls.at(-1)).toBe("Bearer e2e-token-2")
  await expectNoHorizontalScroll(page)
  expect(tab.problems).toEqual([])
})

test("a logout then a login in another tab never wipes that tab's new login", async ({
  page,
}) => {
  await seedSession(page)
  await mockApi(page, flowApi())
  const ends = voiceEnds(page)
  await talk(page, "Private answer to")
  const tab = await otherTab(page)

  await tab.logOutThenIn("e2e-token-2")
  await expect.poll(() => ends.length).toBe(1)
  expect(ends[0]).toMatchObject({ status: 200, body: { workspaceId: "w1" } })
  await expect(page.getByText("Private answer")).toHaveCount(0)
  // Give tab 1 every chance to (wrongly) write back, then check tab 2 is still signed in.
  await page.evaluate(() => new Promise((r) => setTimeout(r, 500)))
  expect(await tab.savedToken()).toBe("e2e-token-2")
  await expectNoHorizontalScroll(page)
  expect(tab.problems).toEqual([])
})

storefrontTest.describe("the storefront preview frame", () => {
  storefrontTest(
    "drops the draft and calls nothing more when another tab switches workspace or logs out",
    async ({ page, api }) => {
      designs(api)
      designs(api, "w2", "Ruaha harvest")
      // The frame alone, in a Flow page with no app code: nothing else can remount it.
      await page.goto("/healthz")
      await page.evaluate((src) => {
        const frame = document.createElement("iframe")
        frame.title = "Shop preview"
        frame.src = src
        frame.style.width = "100%"
        frame.style.height = "600px"
        document.body.append(frame)
      }, `/storefront/preview?revision=${draft}`)
      const frame = await previewFrame(page)
      await expect(
        frame.getByRole("heading", { name: "From farm to your table" })
      ).toBeVisible()
      const before = api.callsTo("GET", `${root()}/preview`).length
      const tab = await otherTab(page)

      await tab.selectWorkspace(["w1", "w2"], "w2")
      await expect(frame.getByText("From farm to your table")).toHaveCount(0)
      await expect(frame.locator("main")).toHaveCount(0)

      await tab.logOut()
      await expect(frame.getByText("From farm to your table")).toHaveCount(0)
      expect(api.callsTo("GET", `${root()}/preview`)).toHaveLength(before)
      expect(api.callsTo("GET", `${root("w2")}/preview`)).toHaveLength(0)
      expect(tab.problems).toEqual([])
    }
  )

  storefrontTest(
    "on the review page: a workspace switch elsewhere reloads for the new workspace, a logout leaves",
    async ({ page, api }) => {
      designs(api)
      designs(api, "w2", "Ruaha harvest")
      const previews: string[] = []
      page.on("request", (r) => {
        const url = new URL(r.url())
        if (url.pathname.endsWith("/storefront/pages/home/preview"))
          previews.push(
            `${url.pathname.split("/")[3]}:${url.searchParams.get("revision_id")}`
          )
      })
      await page.goto("/storefront")
      await page.getByRole("button", { name: revisionButton(2) }).click()
      const frame = await previewFrame(page)
      await expect(
        frame.getByRole("heading", { name: "From farm to your table" })
      ).toBeVisible()
      const tab = await otherTab(page)

      await tab.selectWorkspace(["w1", "w2"], "w2")
      await expect(
        page.getByRole("button", { name: revisionButton(1) })
      ).toBeVisible()
      await expect(page.locator('iframe[title="Shop preview"]')).toHaveCount(0)
      const w1 = api.callsTo("GET", `${root()}/preview`).length

      await page.getByRole("button", { name: revisionButton(1) }).click()
      const next = await previewFrame(page)
      await expect(
        next.getByRole("heading", { name: "Ruaha harvest" })
      ).toBeVisible()
      // Never the old workspace's draft asked for under the new workspace.
      expect(previews).not.toContain(`w2:${draft}`)
      expect(previews).toContain(`w2:${otherDraft}`)
      expect(api.callsTo("GET", `${root()}/preview`)).toHaveLength(w1)

      const calls = api.calls.length
      await tab.logOut()
      await expect(page).toHaveURL(/\/login$/)
      expect(
        api.calls.slice(calls).filter((c) => c.path.includes("/storefront/"))
      ).toHaveLength(0)
      await expectNoHorizontalScroll(page)
      expect(tab.problems).toEqual([])
    }
  )
})
