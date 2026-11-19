import { expectNoRawValues } from "../../src/test"
import {
  approvalsApi,
  errors,
  hex64,
  r1,
  r2,
  r5,
  r6,
} from "../business-approvals/fixtures"
import { emptyOverview, expect, expectFits, openMenu, test } from "./fake-api"

// Sample data from contracts.md §5.7 (mocked API, C5). Approval and NetSuite order are separate.

const R1 = "/v1/workspace/w1/approvals/requests/r1/approve"

test("approves a request: fingerprint and Idempotency-Key sent, order queued apart from approval", async ({
  page,
  api,
}) => {
  approvalsApi(api)
  emptyOverview(api)
  await page.goto("/")
  await openMenu(page)
  await page.getByRole("link", { name: "Purchase approvals" }).click()
  await expect(
    page.getByRole("heading", { name: "Purchase approvals", level: 1 })
  ).toBeVisible()

  await page.getByRole("button", { name: /Odoo order 17/ }).click()
  const card = page.getByTestId("purchase-approval-card")
  await expect(
    card.getByRole("heading", { name: "Odoo order P00017" })
  ).toBeVisible()
  await expect(
    card.getByText("Odoo estimate; NetSuite calculates tax")
  ).toBeVisible()
  await expect(card.getByText("Untaxed")).toBeVisible()
  await expect(card.getByText("€1,512.50").first()).toBeVisible()
  await expect(
    card.getByText("No NetSuite order yet. It starts after approval.")
  ).toBeVisible()
  await expectFits(page)

  await card.getByRole("button", { name: "Approve" }).click()
  await expect(
    page.getByText("Approved. The NetSuite order is queued.")
  ).toBeVisible()
  await expect(card.getByText("Approved", { exact: true })).toBeVisible()
  await expect(
    card.getByTestId("execution-status").getByText("Queued")
  ).toBeVisible()
  await expect(
    card.getByText("Verified in NetSuite", { exact: true })
  ).toHaveCount(0)
  await expect(card.getByRole("button", { name: "Approve" })).toHaveCount(0)

  const call = api.callsTo(
    "POST",
    "/workspace/:ws/approvals/requests/:id/approve"
  )
  expect(call).toHaveLength(1)
  expect(call[0]!.body).toEqual({ fingerprint: hex64("9f2c") })
  expect(call[0]!.headers["idempotency-key"]).toMatch(/^[0-9a-f-]{36}$/)
  await expectNoRawValues(page)
})

test.describe("stale approve", () => {
  test.use({ allow: { responses: [{ url: R1, status: 409 }] } })

  test("a 409 approval_stale says so in words and Reload brings the new version", async ({
    page,
    api,
  }) => {
    approvalsApi(api).on(
      "POST",
      "/workspace/:ws/approvals/requests/:id/approve",
      () => ({
        status: 409,
        body: errors.stale,
      })
    )
    await page.goto("/approvals/purchases")
    await page.getByRole("button", { name: /Odoo order 17/ }).click()
    await page.getByRole("button", { name: "Approve" }).click()

    await expect(
      page.getByText(
        "This request changed after you opened it. Reload to see the new version."
      )
    ).toBeVisible()
    await expect(page.getByText("approval_stale")).toHaveCount(0)
    await expectNoRawValues(page)
    await expectFits(page)

    const reads = api.callsTo(
      "GET",
      "/workspace/:ws/approvals/requests/:id"
    ).length
    await page.getByRole("button", { name: "Reload" }).click()
    await expect(page.getByRole("button", { name: "Reload" })).toHaveCount(0)
    expect(
      api.callsTo("GET", "/workspace/:ws/approvals/requests/:id").length
    ).toBeGreaterThan(reads)
  })
})

test("reject needs a comment before it is sent", async ({ page, api }) => {
  approvalsApi(api)
  await page.goto("/approvals/purchases")
  await page.getByRole("button", { name: /Odoo order 17/ }).click()
  await page.getByRole("button", { name: "Reject" }).click()
  await expect(
    page.getByText("Say why, so the requester knows what to change.")
  ).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/approvals/requests/:id/reject")
  ).toHaveLength(0)
})

test("a blocked request names each missing match and links to the mappings", async ({
  page,
  api,
}) => {
  approvalsApi(api, [r1, r2])
  await page.goto("/approvals/purchases")
  await page.getByRole("button", { name: "All", exact: true }).click()
  await page.getByRole("button", { name: /Odoo order 18/ }).click()

  const card = page.getByTestId("purchase-approval-card")
  await expect(
    card.getByText("This request cannot be decided yet")
  ).toBeVisible()
  await expect(card.getByText("Vendor 43 has no NetSuite match")).toBeVisible()
  await expect(card.getByText("Product 9 has no NetSuite match")).toBeVisible()
  await expect(card.getByRole("button", { name: "Approve" })).toHaveCount(0)
  await expect(
    card.getByRole("link", { name: "Fix in mappings" }).first()
  ).toHaveAttribute("href", "/settings/integrations/p1/mappings?entity=vendor")
  await expectNoRawValues(page)
  await expectFits(page)
})

test("awaiting reconciliation: only the API's actions, link needs an order id and a reason", async ({
  page,
  api,
}) => {
  approvalsApi(api, [r5])
  await page.goto("/approvals/purchases")
  await page.getByRole("button", { name: "All", exact: true }).click()
  await page.getByRole("button", { name: /Odoo order 21/ }).click()

  const exec = page.getByTestId("execution-status")
  await expect(exec.getByText("Checking NetSuite")).toBeVisible()
  await expect(exec.getByText("Not verified in NetSuite")).toBeVisible()
  await expect(
    exec.getByRole("button", { name: "Link existing order" })
  ).toBeVisible()
  await expect(
    exec.getByRole("button", { name: "Close without order" })
  ).toBeVisible()
  await expect(
    exec.getByRole("button", { name: "Allow one more try" })
  ).toHaveCount(0)
  await expectFits(page)

  await exec.getByRole("button", { name: "Link existing order" }).click()
  await exec.getByLabel("NetSuite order id").fill("8812")
  await exec.getByLabel("Reason").fill("Found in NetSuite")
  await exec.getByRole("button", { name: "Link existing order" }).last().click()

  await expect(page.getByText("Saved.", { exact: true })).toBeVisible()
  await expect(exec.getByText("Resolved by hand")).toBeVisible()
  await expect(
    exec.getByText("Linked to an existing order with differences")
  ).toBeVisible()
  await expect(
    exec.getByText("Verified in NetSuite", { exact: true })
  ).toHaveCount(0)
  const call = api.callsTo(
    "POST",
    "/workspace/:ws/workflows/executions/:id/resolve"
  )[0]!
  expect(call.body).toEqual({
    action: "link_existing",
    reason: "Found in NetSuite",
    target_id: "8812",
  })
  expect(call.headers["idempotency-key"]).toBeTruthy()
  await expectNoRawValues(page)
})

test("tax variance: verified order, open issue, accept or keep open", async ({
  page,
  api,
}) => {
  approvalsApi(api, [r6])
  await page.goto("/approvals/purchases")
  await page.getByRole("button", { name: "All", exact: true }).click()
  await page.getByRole("button", { name: /Odoo order 22/ }).click()

  const card = page.getByTestId("purchase-approval-card")
  await expect(card.getByText(/Tax difference · Open issue/)).toBeVisible()
  const exec = card.getByTestId("execution-status")
  await expect(exec.getByText("Created, tax differs")).toBeVisible()
  await expect(
    exec.getByText("Verified in NetSuite", { exact: true })
  ).toBeVisible()
  await expect(
    exec.getByRole("button", { name: "Accept tax difference" })
  ).toBeVisible()
  await expect(
    exec.getByRole("button", { name: "Keep tax difference open" })
  ).toBeVisible()
  await expectNoRawValues(page)
  await expectFits(page)
})

test("nothing to decide is said plainly", async ({ page, api }) => {
  approvalsApi(api, [])
  await page.goto("/approvals/purchases")
  await expect(page.getByText("Nothing to decide")).toBeVisible()
  await expectFits(page)
})
