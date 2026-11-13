import { readFileSync } from "node:fs"
import { resolve } from "node:path"

import type { APIRequestContext, Page } from "@playwright/test"

import {
  apiAs,
  cred,
  expect,
  loginFlow,
  mailTo,
  stackAllow,
  test,
  unique,
  urls,
} from "../src/stack"
import { expectNoHorizontalScroll } from "../src/test"
import { fakeAgent, startText } from "../tests/flow/voice"

import { fakeFlowTransport } from "./flow-voice"

// REAL API dry run, Flow and its preview frame; SIMULATED only the voice side (no live agent):
// the browser's /api/voice calls are answered here with the review Office would propose.

const fixture = JSON.parse(
  readFileSync(
    resolve(
      import.meta.dirname,
      "../../../apps/api/tests/fixtures/storefront-design-v1.json"
    ),
    "utf8"
  )
)

const edit = (expected_version: number, base_revision_id: string | null) => ({
  expected_version,
  base_revision_id,
  changes: [
    { op: "set_theme", theme: "neutral" },
    {
      op: "set_product_grid",
      section_id: "featured-products",
      heading: "Fresh this week",
    },
  ],
})

/** Answers Flow's voice routes so one design edit review appears for this exact body. */
async function proposeEdit(page: Page, body: unknown) {
  await page.route("**/api/voice/readiness", (route) =>
    route.fulfill({ json: { enabled: true, ready: true } })
  )
  await fakeFlowTransport(page)
  await page.route("**/api/voice/tools", async (route) => {
    const sent = route.request().postDataJSON() as { command?: string }
    if (sent.command !== "execute")
      return route.fulfill({ json: { status: "completed", data: [] } })
    return route.fulfill({
      json: {
        status: "requires_confirmation",
        data: { actionId: "0193f6b0-1c2d-7e3f-8a9b-cccccccccccc" },
        review: {
          ticket: `stack-design-${unique()}`,
          summary: "Storefront design edit",
          // As Office builds it (apps/office/lib/voice/actions.ts).
          operationId: "storefront_design_edit",
          method: "POST",
          confirmationCode: "112233",
          confirmationPhrase: "Confirm action 112233",
          target: "Current workspace",
          details: {
            current: {
              changes: [
                {
                  field: "heading",
                  label: fixture.sections[1].props.heading,
                  before: fixture.sections[1].props.heading,
                  after: "Fresh this week",
                },
              ],
            },
            changes: body,
          },
          expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
        },
      },
    })
  })
  const agent = await fakeAgent(page)
  await page.goto(`${urls.flow}/`)
  await startText(page)
  agent.call("storefront_design_edit", { body })
}

/** A fresh workspace (the shared seed's design stays untouched) with one product and one draft. */
async function ownerWithDraft(request: APIRequestContext) {
  const name = `e2e-flow-p3-${unique()}`
  const login = {
    app: "flow (design p3)",
    role: "owner",
    login: `${name}@e2e.test`,
    secret: `E2e!${unique()}Aa1`,
    path: "/login",
  }
  const signup = await request.post(`${urls.api}/v1/create-workspace`, {
    data: {
      first_name: "Flow",
      last_name: "Owner",
      email: login.login,
      password: login.secret,
      workspace_name: name,
    },
  })
  expect(signup.status()).toBe(201)
  const link = (await mailTo(login.login)).match(
    /https?:\/\/\S+\/v1\/verify-email\/\S+/
  )?.[0]
  expect(link).toBeTruthy()
  await request.get(link!, { maxRedirects: 0 })
  const owner = await apiAs(request, login)
  const product = `Harvest ${unique()}`
  await owner.post(owner.ws("/products"), { name: product, price: "4.00" })
  const path = owner.ws("/storefront/pages/home")
  const save = async (expected_version: number) => {
    const saved = await request.post(`${urls.api}/v1${path}/revisions`, {
      headers: {
        Authorization: `Bearer ${owner.token}`,
        "Idempotency-Key": unique(),
      },
      data: { expected_version, document: fixture },
    })
    expect(saved.ok(), `draft: ${saved.status()}`).toBe(true)
    return (await saved.json()).data as {
      version: number
      draft_revision_id: string
    }
  }
  const first = await save(0)
  return { login, owner, product, path, save, first }
}

const dryRuns = (page: Page) => {
  const seen: number[] = []
  page.on("response", (r) => {
    if (r.url().includes("/edits/preview")) seen.push(r.status())
  })
  return seen
}

for (const [label, viewport] of [
  ["phone 375", { width: 375, height: 812 }],
  ["desktop 1280", { width: 1280, height: 800 }],
] as const) {
  test.describe(label, () => {
    test.use({ viewport })

    test("the proposal frame shows the API's dry run of the exact edit, with the live catalog", async ({
      page,
      request,
    }) => {
      test.setTimeout(120_000)
      const { login, owner, product, path, first } =
        await ownerWithDraft(request)
      const statuses = dryRuns(page)
      await loginFlow(page, login)
      await proposeEdit(page, edit(first.version, first.draft_revision_id))
      await expect(
        page.locator('iframe[title="Proposed shop preview"]')
      ).toBeVisible()
      await expect
        .poll(() =>
          page
            .frames()
            .some((f) => f.url().includes("/storefront/preview?proposal=1"))
        )
        .toBe(true)
      const frame = page
        .frames()
        .find((f) => f.url().includes("/storefront/preview?proposal=1"))!
      await expect(
        frame.getByRole("heading", { name: "Fresh this week" })
      ).toBeVisible()
      await expect(frame.getByText(product)).toBeVisible()
      await expectNoHorizontalScroll(page)
      expect(statuses).toEqual([200])
      // A dry run stores nothing.
      const state = await owner.get<{
        version: number
        draft_revision_id: string
      }>(path)
      expect(state).toMatchObject({
        version: first.version,
        draft_revision_id: first.draft_revision_id,
      })
    })
  })
}

test.describe("refusals", () => {
  // The guard allows exact absolute URLs only (no baseURL here), and the moved draft lives in a
  // workspace made at run time: both refused dry runs are known once beforeAll has made them.
  const allowed: { url: string; status: number }[] = []
  let moved: Awaited<ReturnType<typeof ownerWithDraft>>
  test.beforeAll(async ({ playwright }) => {
    test.setTimeout(120_000)
    const request = await playwright.request.newContext()
    moved = await ownerWithDraft(request)
    await moved.save(moved.first.version) // someone else saved meanwhile
    const member = await apiAs(request, cred("field (RUAHA)", "ops"))
    allowed.push(
      { url: `${urls.flow}/v1${moved.path}/edits/preview`, status: 409 },
      {
        url: `${urls.flow}/v1${member.ws("/storefront/pages/home")}/edits/preview`,
        status: 403,
      }
    )
    await request.dispose()
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({ ...stackAllow, responses: [...stackAllow.responses, ...allowed] }),
  })

  test("a draft that moved: the API answers 409, no preview, approval held", async ({
    page,
    request,
  }) => {
    void request
    const { login, first } = moved
    const statuses = dryRuns(page)
    await loginFlow(page, login)
    await proposeEdit(page, edit(first.version, first.draft_revision_id))
    await expect(
      page
        .getByText("The storefront changed after this review was prepared.")
        .first()
    ).toBeVisible()
    await expect(page.locator("iframe")).toHaveCount(0)
    expect(statuses).toEqual([409])
    await expect(
      page.getByRole("button", { name: "Yes, do it" }).first()
    ).toBeDisabled()
  })

  test("a member is refused by the API and sees no preview", async ({
    page,
    request,
  }) => {
    const ops = cred("field (RUAHA)", "ops")
    const statuses = dryRuns(page)
    await loginFlow(page, ops)
    await proposeEdit(page, edit(1, null))
    await expect(
      page.getByText("This preview is not available.").first()
    ).toBeVisible()
    await expect(page.locator("iframe")).toHaveCount(0)
    expect(statuses).toEqual([403])
    // Directly, too.
    const member = await apiAs(request, ops)
    const refused = await request.post(
      `${urls.api}/v1${member.ws("/storefront/pages/home/edits/preview")}`,
      {
        headers: { Authorization: `Bearer ${member.token}` },
        data: edit(1, null),
      }
    )
    expect(refused.status()).toBe(403)
  })
})
