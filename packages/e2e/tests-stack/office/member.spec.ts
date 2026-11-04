import { expectNoRawValues } from "../../src/test"
import { cred, expect, loginOffice, test } from "../../src/stack"

// A workspace member without office rights (a RUAHA field officer): the overview asks only for
// what they may read, so the guard sees no 403 at all, and the tiles they may not see are gone.
test("office overview for a member without office rights: no refused request, only their tiles", async ({ page }) => {
  await loginOffice(page, cred("field (RUAHA)", "field_officer"))
  await expect(page.getByText("Batches by status")).toBeVisible()
  await expect(page.getByText("Needs attention")).toBeVisible()
  for (const title of ["Orders to pick", "Recent orders", "Sales this week", "Till today", "Products below zero"])
    await expect(page.getByText(title, { exact: true }), title).toHaveCount(0)
  await expectNoRawValues(page)
})

test("office overview for the owner still shows every tile", async ({ page }) => {
  await loginOffice(page, cred("office (AFM)", "owner"))
  for (const title of ["Orders to pick", "Recent orders", "Sales this week", "Till today", "Batches by status"])
    await expect(page.getByText(title, { exact: true }), title).toBeVisible()
})
