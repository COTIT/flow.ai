import { readFileSync } from "node:fs"
import { resolve } from "node:path"

import type { Page } from "@playwright/test"

import {
  apiAs,
  cred,
  expect,
  loginFlow,
  mailTo,
  test,
  unique,
  urls,
} from "../src/stack"
import { expectNoHorizontalScroll } from "../src/test"

const fixture = JSON.parse(
  readFileSync(
    resolve(
      import.meta.dirname,
      "../../../apps/api/tests/fixtures/storefront-design-v1.json"
    ),
    "utf8"
  )
)

async function previewFrame(page: Page) {
  await expect
    .poll(() =>
      page.frames().some((f) => f.url().includes("/storefront/preview"))
    )
    .toBe(true)
  return page.frames().find((f) => f.url().includes("/storefront/preview"))!
}

/** Its own workspace, so the shared seed's design stays untouched. */
test("an owner reviews an API draft in Flow's device frame and publishes it", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000)
  const name = `e2e-flow-design-${unique()}`
  const ownerLogin = {
    app: "flow (design)",
    role: "owner",
    login: `${name}@e2e.test`,
    secret: `E2e!${unique()}Aa1`,
    path: "/login",
  }
  const signup = await request.post(`${urls.api}/v1/create-workspace`, {
    data: {
      first_name: "Flow",
      last_name: "Owner",
      email: ownerLogin.login,
      password: ownerLogin.secret,
      workspace_name: name,
    },
  })
  expect(signup.status()).toBe(201)
  const link = (await mailTo(ownerLogin.login)).match(
    /https?:\/\/\S+\/v1\/verify-email\/\S+/
  )?.[0]
  expect(link).toBeTruthy()
  await request.get(link!, { maxRedirects: 0 })
  const owner = await apiAs(request, ownerLogin)
  const product = `Harvest ${unique()}`
  await owner.post(owner.ws("/products"), { name: product, price: "4.00" })
  const path = owner.ws("/storefront/pages/home")
  const created = await request.post(`${urls.api}/v1${path}/revisions`, {
    headers: {
      Authorization: `Bearer ${owner.token}`,
      "Idempotency-Key": unique(),
    },
    data: { expected_version: 0, document: fixture },
  })
  expect(created.ok(), `draft: ${created.status()}`).toBe(true)
  const draft = (await created.json()).data.draft_revision_id as string

  await loginFlow(page, ownerLogin)
  await page.goto(`${urls.flow}/storefront`)
  await page
    .getByRole("button", { name: /^Preview revision 1, saved / })
    .click()
  const region = page.getByRole("region", { name: "Storefront preview" })
  await expect(region.getByText("Draft — not live yet")).toBeVisible()
  const frame = await previewFrame(page)
  await expect(
    frame.getByRole("heading", { name: fixture.sections[0].props.heading })
  ).toBeVisible()
  // Live catalog data through the authorized preview.
  await expect(frame.getByText(product)).toBeVisible()
  await page.getByRole("button", { name: "Desktop", exact: true }).click()
  await expect
    .poll(() =>
      frame.evaluate(() => ({
        width: window.innerWidth,
        columns: getComputedStyle(
          document.querySelector("ul")!
        ).gridTemplateColumns.split(" ").length,
      }))
    )
    .toEqual({ width: 1280, columns: 3 })
  await expectNoHorizontalScroll(page)

  await region.getByRole("button", { name: "Publish", exact: true }).click()
  // F5: the studio's publish card (live → proposed) asks for the confirmation.
  await page
    .getByRole("region", { name: "Publish storefront revision?" })
    .getByRole("button", { name: "Confirm publish" })
    .click()
  await expect(
    page.getByRole("region", { name: "Storefront published" })
  ).toBeVisible()
  const state = await owner.get<{ published_revision_id: string | null }>(path)
  expect(state.published_revision_id).toBe(draft)
})

test("a member without administration rights sees no designs, and the API refuses them", async ({
  page,
  request,
}) => {
  const ops = cred("field (RUAHA)", "ops")
  const member = await apiAs(request, ops)
  const refused = await request.get(
    `${urls.api}/v1${member.ws("/storefront/pages/home")}`,
    { headers: { Authorization: `Bearer ${member.token}` } }
  )
  expect(refused.status()).toBe(403)

  const designCalls: string[] = []
  page.on("request", (r) => {
    if (r.url().includes("/storefront/pages/")) designCalls.push(r.url())
  })
  await loginFlow(page, ops)
  await page.goto(`${urls.flow}/storefront`)
  await expect(
    page.getByText(
      "Only workspace owners and admins can review the storefront."
    )
  ).toBeVisible()
  await expect(page.locator("iframe")).toHaveCount(0)
  expect(designCalls).toEqual([])
})
