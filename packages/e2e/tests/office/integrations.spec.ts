import { expectNoRawValues } from "../../src/test"
import { c3, errors, integrationsApi } from "../business-approvals/fixtures"
import { expect, expectFits, test } from "./fake-api"

// Sample data from contracts.md §5.7 (mocked API, C5). Secrets are write-only.

test("connections show a mask, never a secret, and the probe in words", async ({
  page,
  api,
}) => {
  integrationsApi(api)
  await page.goto("/settings/integrations")
  await expect(
    page.getByRole("heading", { name: "Integrations", level: 1 })
  ).toBeVisible()
  await expect(page.getByText("Odoo main", { exact: true })).toBeVisible()
  await expect(page.getByText("All 1 checks passed")).toBeVisible()
  await expect(page.getByText("Login refused")).toBeVisible()
  await expect(page.getByText(c3.last_probe[0]!.detail)).toBeVisible()
  await expect(page.getByLabel("Stored. Leave as is to keep it.")).toHaveCount(
    2
  )
  await expect(page.getByText("Odoo's approval settings changed")).toBeVisible()
  await expectNoRawValues(page, { allow: ["********"] })
  await expectFits(page)
})

test("adding a connection sends the secret once and shows only the mask after", async ({
  page,
  api,
}) => {
  integrationsApi(api)
  await page.goto("/settings/integrations")
  await page.getByRole("button", { name: "Add connection" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Name").fill("Odoo test")
  await dialog.getByLabel("Odoo address").fill("https://test.odoo.com")
  await expect(dialog.getByLabel("API key")).toHaveAttribute("type", "password")
  await dialog.getByLabel("API key").fill("sk-very-secret")
  await dialog.getByRole("button", { name: "Save" }).click()

  await expect(page.getByText("Connection added.")).toBeVisible()
  const call = api.callsTo(
    "POST",
    "/workspace/:ws/integrations/connections"
  )[0]!
  expect(call.body).toMatchObject({
    provider: "odoo",
    label: "Odoo test",
    config: { base_url: "https://test.odoo.com", database: null },
    secrets: { api_key: "sk-very-secret" },
  })
  expect(call.path).not.toContain("secret")
  await expect(page.getByText("Odoo test")).toBeVisible()
  await expect(page.getByText("sk-very-secret")).toHaveCount(0)
  expect(await page.content()).not.toContain("sk-very-secret")
  await expectFits(page)
})

test("editing without a new secret sends the mask back, so the stored one is kept", async ({
  page,
  api,
}) => {
  integrationsApi(api)
  await page.goto("/settings/integrations")
  await page.getByRole("button", { name: "Edit: Odoo main" }).click()
  const secret = page.getByRole("dialog").getByLabel("API key")
  await expect(secret).toHaveValue("")
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
  await expect(page.getByText("Connection saved.")).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/integrations/connections/:id")[0]!.body
  ).toMatchObject({ secrets: { api_key: "********" } })
})

test.describe("field errors", () => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/integrations/connections", status: 422 },
      ],
    },
  })
  test("a 422 shows each field's words next to the field", async ({
    page,
    api,
  }) => {
    integrationsApi(api).on(
      "POST",
      "/workspace/:ws/integrations/connections",
      () => ({
        status: 422,
        body: errors.connection422,
      })
    )
    await page.goto("/settings/integrations")
    await page.getByRole("button", { name: "Add connection" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Name").fill("x")
    await dialog.getByLabel("Odoo address").fill("https://x.test")
    await dialog.getByLabel("API key").fill("k")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(
      dialog.getByText("The base url must be a valid URL.")
    ).toBeVisible()
    await expect(dialog.getByText("The label field is required.")).toBeVisible()
  })
})

test("test shows the probe; mappings filter by kind", async ({ page, api }) => {
  integrationsApi(api)
  await page.goto("/settings/integrations")
  await page.getByRole("button", { name: "Test: NS prod" }).click()
  await expect(page.getByText("Test finished.")).toBeVisible()

  await page.getByRole("link", { name: "Mappings" }).click()
  await expect(
    page.getByRole("heading", { name: "Mappings", level: 1 })
  ).toBeVisible()
  await expect(
    page.getByText("Azure Interior → Azure Interior Ltd")
  ).toBeVisible()
  await page.goto("/settings/integrations/p1/mappings?entity=product")
  await expect(page.getByText("No mappings yet")).toBeVisible()
  await page.getByLabel("Odoo id").fill("9")
  await page.getByLabel("NetSuite id").fill("501")
  await page.getByRole("button", { name: "Add mapping" }).click()
  await expect(page.getByText("Mapping saved.")).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/integrations/pairs/:pair/mappings")[0]!
      .body
  ).toEqual({ entity: "product", source_id: "9", target_id: "501" })
  await expectNoRawValues(page)
  await expectFits(page)
})
