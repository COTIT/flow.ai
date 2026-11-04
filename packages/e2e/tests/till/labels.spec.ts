import { expectNoRawValues } from "../../src/test"
import { expect, seedTill, test, WS } from "./fake-till"

// A refused call reads in the cashier's language: a refusal the till knows in its own words,
// any other with a translated title and the API's English below as the detail.
test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: `/v1/workspace/${WS}/till/shift/open`, status: 409 }],
    },
  })

  for (const [said, title] of [
    ["A shift is already open on this register.", "A shift is already open on this register."],
    ["The register is locked by the office.", "The drawer could not be changed."],
  ] as const)
    test(`a refused shift opening reads "${title}"`, async ({ page, api }) => {
      api
        .on("GET", "/workspace/:ws/till/shift", () => ({ data: null }))
        .on("POST", "/workspace/:ws/till/shift/open", () => ({
          status: 409,
          body: { message: said },
        }))
      await seedTill(page)
      await page.goto("/drawer")
      await page.getByLabel("Pieces of €10.00").fill("1")
      await page.getByRole("button", { name: "Open shift" }).click()
      const alert = page.getByRole("alert").filter({ hasText: title })
      await expect(alert).toBeVisible()
      if (said !== title) await expect(alert).toContainText(said)
      await expectNoRawValues(page)
    })
})
