import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"

import { expectNoRawValues, test as guarded } from "../../src/test"
import { approvalsApi, hex64, r1, r2 } from "../business-approvals/fixtures"
import { expect, test } from "./storefront-api"

// Sample data from contracts.md §5.7 (mocked API, C5). Business approvals in Flow are a route of
// their own: one primary action (Approve), and the agent-task approval scene left as it was.

test("approves from Flow with one primary action; the order is shown apart from the approval", async ({
  page,
  api,
}) => {
  approvalsApi(api, [r1])
  await page.goto("/approvals/purchases")
  await expect(
    page.getByRole("heading", { name: "Purchase approvals", level: 1 })
  ).toBeVisible()
  await page.getByRole("button", { name: /Odoo order 17/ }).click()

  const card = page.getByTestId("purchase-approval-card")
  await expect(
    card.getByText("Odoo estimate; NetSuite calculates tax")
  ).toBeVisible()
  // Approve is the one filled button; send back and reject are secondary.
  const approve = card.getByRole("button", { name: "Approve" })
  await expect(approve).toHaveClass(/\bbg-primary\b/)
  for (const name of ["Send back", "Reject"])
    await expect(card.getByRole("button", { name })).not.toHaveClass(
      /\bbg-primary\b/
    )

  await approve.click()
  await expect(
    page.getByText("Approved. The NetSuite order is queued.")
  ).toBeVisible()
  await expect(
    card.getByTestId("execution-status").getByText("Queued")
  ).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/approvals/requests/:id/approve")[0]!
      .body
  ).toEqual({
    fingerprint: hex64("9f2c"),
  })
  await expectNoRawValues(page)
})

test("a blocked request links each missing match to Office's mappings", async ({
  page,
  api,
}) => {
  approvalsApi(api, [r2])
  await page.goto("/approvals/purchases")
  await page.getByRole("button", { name: "All", exact: true }).click()
  await page.getByRole("button", { name: /Odoo order 18/ }).click()
  await expect(page.getByText("Vendor 43 has no NetSuite match")).toBeVisible()
  await expect(
    page.getByRole("link", { name: "Fix in mappings" }).first()
  ).toHaveAttribute(
    "href",
    "https://office.genesy.test/settings/integrations/p1/mappings?entity=vendor"
  )
  await expectNoRawValues(page)
})

// T37: the agent-task approval scene and the dock are byte for byte as on the C5 base
// (6d22d5ef1); their behaviour stays covered by task-review.spec.ts and offline-approve.spec.ts.
guarded("T37: the agent-task approval scene is unchanged", () => {
  const blob = (path: string) => {
    const data = readFileSync(
      new URL(`../../../../apps/flow/${path}`, import.meta.url)
    )
    return createHash("sha1")
      .update(`blob ${data.length}\0`)
      .update(data)
      .digest("hex")
  }
  expect(blob("components/task/approval-scene.tsx")).toBe(
    "57d2e3564089637374a440aee22fbf611c7c0a8f"
  )
  expect(blob("components/frame/dock.tsx")).toBe(
    "d221adef9c95f8047b49aed534a410d412bb256f"
  )
})
