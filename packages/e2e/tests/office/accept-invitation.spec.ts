import type { Page } from "@playwright/test"

import { test } from "../../src/test"
import { officeApi } from "./api"
import {
  expect,
  expectFits,
  fakeApi,
  test as signedIn,
  type FakeApi,
} from "./fake-api"

// A code nobody else uses, so a search for it in storage or the console finds only a leak.
const CODE = "InvQ7x9Zk2"
const page_ = (code = CODE) => `/accept-invitation?code=${code}`

const status = (over: Record<string, unknown> = {}) => ({
  data: {
    id: "inv-1",
    email: "amina@example.com",
    invited_on: "2026-11-01T08:00:00Z",
    new_user: true,
    status: "pending",
    // The slug and the company name: the page shows display_name.
    workspace_name: "ilundo-honey",
    display_name: "Ilundo Honey Co.",
    ...over,
  },
})

/** The overview's calls (where a finished invite lands), on the stateful fake. */
function overview(api: FakeApi) {
  for (const [key, reply] of Object.entries(officeApi())) {
    const [method, path] = key.split(" ") as [string, string]
    api.on(method, path, () => ({
      status: reply.status ?? 200,
      body: reply.body,
    }))
  }
}

/** Every console line, to prove the code never reaches it. */
function consoleLines(page: Page) {
  const lines: string[] = []
  page.on("console", (m) => lines.push(m.text()))
  return lines
}

/** The code is in no storage, no cookie and no console line. */
async function expectCodeKeptOut(page: Page, lines: string[]) {
  const stored = await page.evaluate(() =>
    JSON.stringify({ ...localStorage, ...sessionStorage, c: document.cookie })
  )
  expect(stored).not.toContain(CODE)
  const cookies = await page.context().cookies()
  expect(JSON.stringify(cookies)).not.toContain(CODE)
  expect(lines.filter((l) => l.includes(CODE))).toEqual([])
}

test("no code: the link is incomplete", async ({ page }) => {
  await fakeApi(page)
  await page.goto("/accept-invitation")
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "This invite link is incomplete" })
  ).toBeVisible()
  await expect(page.getByRole("img", { name: "Genesy" })).toBeVisible()
  await expectFits(page)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/invites/get-invite-status/nope", status: 404 }],
    },
  })

  test("an unknown code is not valid", async ({ page }) => {
    const api = await fakeApi(page)
    api.on("GET", "/invites/get-invite-status/:code", () => ({
      status: 404,
      body: { message: "Not found." },
    }))
    await page.goto(page_("nope"))
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "This invite link is not valid" })
    ).toBeVisible()
  })
})

for (const [state, text] of [
  ["expired", "This invite has expired"],
  ["accepted", "This invite was already accepted"],
  ["denied", "This invite was declined"],
] as const)
  test(`a ${state} invite says so and links to login`, async ({ page }) => {
    const api = await fakeApi(page)
    api.on("GET", "/invites/get-invite-status/:code", () =>
      status({ status: state })
    )
    await page.goto(page_())
    await expect(page.getByText(text)).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Go to login" })
    ).toHaveAttribute("href", "/login")
    await expect(
      page.getByRole("button", { name: "Create account" })
    ).toHaveCount(0)
    await expectFits(page)
  })

test("new user: register signs in and lands on the overview", async ({
  page,
}) => {
  const lines = consoleLines(page)
  const api = await fakeApi(page)
  overview(api)
  api
    .on("GET", "/invites/get-invite-status/:code", () => status())
    .on("POST", "/register", () => ({
      status: 201,
      body: {
        data: {
          access_token: "new-token",
          token_type: "Bearer",
          workspace_ids: ["w1"],
        },
      },
    }))
  await page.goto(page_())
  await expect(
    page.getByRole("heading", { name: "Join Ilundo Honey Co." })
  ).toBeVisible()
  await expect(page.getByLabel("Email")).toHaveValue("amina@example.com")
  await expect(page.getByLabel("Email")).toHaveAttribute("readonly", "")
  await expect(
    page.getByText(
      "At least 8 characters with upper and lower case, a number and a symbol."
    )
  ).toBeVisible()
  await expect(page.getByLabel("Password")).toHaveAttribute(
    "autocomplete",
    "new-password"
  )
  await expectFits(page)
  await page.getByLabel("First name").fill("Amina")
  await page.getByLabel("Last name").fill("Mushi")
  await page.getByLabel("Password").fill("Honey!2026")
  await page.getByRole("button", { name: "Create account" }).click()

  await expect(page).toHaveURL("/")
  await expect(
    page.getByRole("heading", { level: 1, name: "Overview" })
  ).toBeVisible()
  expect(api.callsTo("POST", "/register")[0]?.body).toEqual({
    email: "amina@example.com",
    first_name: "Amina",
    last_name: "Mushi",
    password: "Honey!2026",
    invite_code: CODE,
  })
  await expectCodeKeptOut(page, lines)
})

test.describe(() => {
  test.use({ allow: { responses: [{ url: "/v1/register", status: 422 }] } })

  test("new user: a 422 shows next to each field", async ({ page }) => {
    const api = await fakeApi(page)
    api
      .on("GET", "/invites/get-invite-status/:code", () => status())
      .on("POST", "/register", () => ({
        status: 422,
        body: {
          message: "The given data was invalid.",
          errors: {
            first_name: [
              "The first name may not be longer than 55 characters.",
            ],
            password: ["The password must contain at least one symbol."],
          },
        },
      }))
    await page.goto(page_())
    await page.getByLabel("First name").fill("A".repeat(55))
    await page.getByLabel("Last name").fill("Mushi")
    await page.getByLabel("Password").fill("Honey2026")
    await page.getByRole("button", { name: "Create account" }).click()
    await expect(
      page.getByText("The password must contain at least one symbol.")
    ).toBeVisible()
    await expect(
      page.getByText("The first name may not be longer than 55 characters.")
    ).toBeVisible()
    await expect(page.getByLabel("Password")).toHaveAttribute(
      "aria-invalid",
      "true"
    )
    await expect(page.getByLabel("First name")).toHaveAttribute(
      "aria-invalid",
      "true"
    )
    await expect(page).toHaveURL(page_())
  })
})

signedIn(
  "existing user, signed in: accept joins, then a fresh login opens the new account",
  async ({ page, api }) => {
    const lines = consoleLines(page)
    api
      .on("GET", "/invites/get-invite-status/:code", () =>
        status({ new_user: false })
      )
      .on("POST", "/invites", () => ({ message: "Invite accepted." }))
    await page.goto(page_())
    await expect(
      page.getByRole("heading", { name: "Join Ilundo Honey Co." })
    ).toBeVisible()
    await expect(page.getByRole("button", { name: "Decline" })).toBeVisible()
    await expectFits(page)
    await page.getByRole("button", { name: "Accept" }).click()
    // The saved login only knows the old accounts; logging in again brings the new one.
    await expect(page.getByText("You joined Ilundo Honey Co.")).toBeVisible()
    await expect(page.getByRole("button", { name: "Decline" })).toHaveCount(0)
    expect(api.callsTo("POST", "/invites")[0]?.body).toEqual({
      accept: true,
      invite_code: CODE,
    })
    await expectFits(page)
    await page.getByRole("button", { name: "Log in again" }).click()
    await expect(page).toHaveURL("/login")
    expect(api.callsTo("POST", "/logout")).toHaveLength(1)
    await expectCodeKeptOut(page, lines)
  }
)

signedIn("existing user, signed in: decline says so", async ({ page, api }) => {
  api
    .on("GET", "/invites/get-invite-status/:code", () =>
      status({ new_user: false })
    )
    .on("POST", "/invites", () => ({ message: "Invite declined." }))
  await page.goto(page_())
  await page.getByRole("button", { name: "Decline" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Invite declined" })
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Accept" })).toHaveCount(0)
  expect(api.callsTo("POST", "/invites")[0]?.body).toEqual({
    accept: false,
    invite_code: CODE,
  })
  await expect(page).toHaveURL(page_())
})

test("existing user, signed out: log in, then back to the invite", async ({
  page,
}) => {
  const lines = consoleLines(page)
  const api = await fakeApi(page)
  api
    .on("GET", "/invites/get-invite-status/:code", () =>
      status({ new_user: false })
    )
    .on("POST", "/login", () => ({
      data: {
        access_token: "e2e-token",
        token_type: "Bearer",
        workspace_ids: ["w1"],
      },
    }))
  await page.goto(page_())
  await expect(page.getByText("Log in to accept")).toBeVisible()
  const login = page.getByRole("link", { name: "Log in" })
  await expect(login).toHaveAttribute(
    "href",
    `/login?next=${encodeURIComponent(page_())}`
  )
  await expect(page.getByRole("button", { name: "Accept" })).toHaveCount(0)
  await expectFits(page)
  await login.click()
  await expect(page).toHaveURL(/\/login\?next=/)
  await page.getByLabel("Email").fill("amina@example.com")
  await page.getByLabel("Password").fill("e2e-password-only")
  await page.getByRole("button", { name: "Log in" }).click()
  await expect(page).toHaveURL(page_())
  await expect(page.getByRole("button", { name: "Accept" })).toBeVisible()
  await expectCodeKeptOut(page, lines)
})

for (const next of ["//evil.com", "/%2F%2Fevil.com", "https://evil.com"])
  test(`login with next=${next} goes to the overview`, async ({ page }) => {
    const api = await fakeApi(page)
    overview(api)
    api.on("POST", "/login", () => ({
      data: {
        access_token: "e2e-token",
        token_type: "Bearer",
        workspace_ids: ["w1"],
      },
    }))
    await page.goto(`/login?next=${encodeURIComponent(next)}`)
    await page.getByLabel("Email").fill("e2e@genesy.test")
    await page.getByLabel("Password").fill("e2e-password-only")
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(page).toHaveURL("/")
    await expect(
      page.getByRole("heading", { level: 1, name: "Overview" })
    ).toBeVisible()
  })
