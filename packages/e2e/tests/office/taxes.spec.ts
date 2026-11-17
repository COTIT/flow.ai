import { expectNoRawValues } from "../../src/test"
import { expect, expectFits, paged, test, type FakeApi } from "./fake-api"

const profile = (over: Record<string, unknown> = {}) => ({
  id: "tp-1",
  workspace_id: "w1",
  lineage_id: "tp-1",
  revision: 1,
  corrects_id: null,
  superseded_by_id: null,
  country: "TZ",
  jurisdiction_scope: null,
  effective_from: "2026-07-01",
  effective_to: null,
  transaction_types: ["sale", "purchase"],
  product_categories: [] as string[],
  components: [{ code: "VAT", name: "VAT", kind: "percentage", rate: "18.00000000", recoverable: true }],
  exemptions: [] as Record<string, unknown>[],
  tax_point: "invoice",
  evidence_ref: "Finance Act 2026",
  state: "draft",
  version: 1,
  verified_by: null,
  verified_at: null,
  allowed_actions: ["update", "verify"],
  created_by: "u1",
  created_at: "2026-11-10T08:00:00Z",
  updated_at: "2026-11-10T08:00:00Z",
  ...over,
})

const verified = (over: Record<string, unknown> = {}) =>
  profile({
    state: "verified",
    version: 2,
    verified_by: "u1",
    verified_at: "2026-11-11T08:00:00Z",
    allowed_actions: ["correct"],
    ...over,
  })

function taxes(api: FakeApi, list: ReturnType<typeof profile>[]) {
  const state = { list }
  api
    .on("GET", "/workspace/:ws/tax-profiles", ({ query }) =>
      paged(state.list.filter((p) => !query.get("state") || p.state === query.get("state")))
    )
    .on("GET", "/workspace/:ws/tax-profiles/:id", ({ params }) => ({
      data: state.list.find((p) => p.id === params[1]),
    }))
    .on("GET", "/workspace/:ws/erp/product-categories", () =>
      paged([{ id: "c-1", product_category_id: "pc-honey", name: "Honey" }], 1, 100)
    )
  return state
}

test("lists profiles with rates, state and dates, and filters by state", async ({ page, api }) => {
  taxes(api, [verified(), profile({ id: "tp-2", lineage_id: "tp-2", country: "KE", jurisdiction_scope: "KE-30" })])
  await page.goto("/settings/taxes")
  const tz = page.getByRole("row", { name: /Tanzania/ })
  await expect(tz).toContainText("VAT 18%")
  await expect(tz).toContainText("Verified")
  await expect(page.getByRole("row", { name: /Kenya \(KE-30\)/ })).toContainText("Draft")
  await expectFits(page)
  await expectNoRawValues(page, { allow: [/\bKE-30\b/] })

  await page.getByLabel("Status").selectOption("draft")
  await expect(page).toHaveURL(/state=draft/)
  await expect(page.getByRole("row", { name: /Tanzania/ })).toHaveCount(0)
})

test("an empty list says so", async ({ page, api }) => {
  taxes(api, [])
  await page.goto("/settings/taxes")
  await expect(page.getByText("No tax profiles yet.")).toBeVisible()
  await expectFits(page)
})

test.describe("a draft", () => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/tax-profiles", status: 422 },
        { url: "/v1/workspace/w1/tax-profiles/tp-1/verify", status: 422 },
      ],
    },
  })

  test("is created with a rate and a category exemption, then verified through its review", async ({
    page,
    api,
  }) => {
    const state = taxes(api, [])
    let tries = 0
    api.on("POST", "/workspace/:ws/tax-profiles", ({ body }) => {
      tries += 1
      if (tries === 1)
        return {
          status: 422,
          body: { message: "The given data was invalid.", errors: { "payload.components.0.rate": ["Use 0 to 100."] } },
        }
      const p = (body as { payload: Record<string, unknown> }).payload
      state.list = [profile({ ...p, exemptions: p.exemptions })]
      return { status: 201, body: { data: state.list[0] } }
    })
    await page.goto("/settings/taxes/new")
    await page.getByLabel("Country (two letters, e.g. TZ)").fill("tz")
    await page.getByLabel("In force from").fill("2026-07-01")
    await page.getByRole("checkbox", { name: "Purchases", exact: true }).click()
    await page.getByLabel("Code", { exact: true }).fill("VAT")
    await page.getByLabel("Name", { exact: true }).fill("VAT")
    await page.getByLabel("Rate (%)").fill("180")
    await page.getByRole("button", { name: "Add exemption" }).click()
    await page.getByLabel("Category", { exact: true }).selectOption("pc-honey")
    await page.getByLabel("Reason").fill("Unprocessed farm produce")
    await page.getByLabel("Evidence (law, notice or letter)").nth(1).fill("VAT Act s.6")
    await expectFits(page)
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page.getByText("Use 0 to 100.")).toBeVisible()
    await page.getByLabel("Rate (%)").fill("18")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page).toHaveURL(/\/settings\/taxes\/tp-1/)
    await expect(page.getByText("Tax profile saved as draft.")).toBeVisible()
    await expect(page.getByRole("region", { name: "Exemptions" })).toContainText("Honey")
    const sent = api.callsTo("POST", "/workspace/:ws/tax-profiles").at(-1)!
    expect(sent.body).toMatchObject({
      payload: {
        country: "TZ",
        jurisdiction_scope: null,
        transaction_types: ["sale", "purchase"],
        product_categories: [],
        tax_point: "invoice",
        components: [{ code: "VAT", name: "VAT", kind: "percentage", rate: "18", recoverable: true }],
        exemptions: [{ product_category_id: "pc-honey", reason: "Unprocessed farm produce", evidence_ref: "VAT Act s.6" }],
        corrects_id: null,
      },
    })

    api.on("POST", "/workspace/:ws/tax-profiles/:id/verify/review", () => ({
      data: { profile: state.list[0], review_fingerprint: "fp-tax" },
    }))
    let verifies = 0
    api.on("POST", "/workspace/:ws/tax-profiles/:id/verify", () => {
      verifies += 1
      if (verifies === 1)
        return { status: 422, body: { message: "tax_profile_overlap", code: "tax_profile_overlap" } }
      state.list = [verified({ exemptions: state.list[0]!.exemptions })]
      return { data: state.list[0] }
    })
    const next = page.getByRole("region", { name: "Next step" })
    await next.getByRole("button", { name: "Verify" }).click()
    const dialog = page.getByRole("dialog")
    await expect(dialog).toContainText("VAT")
    await expect(dialog).toContainText("18%")
    await dialog.getByLabel("Evidence (law, notice or letter)").fill("Finance Act 2026")
    await dialog.getByRole("button", { name: "Verify" }).click()
    await expect(dialog.getByText("Another verified profile already covers")).toBeVisible()
    await dialog.getByRole("button", { name: "Verify" }).click()
    await expect(page.getByRole("status").filter({ hasText: "Tax profile verified." })).toBeVisible()
    await expect(next.getByRole("link", { name: "Make a correction" })).toBeVisible()
    await expect(next.getByRole("link", { name: "Edit draft" })).toHaveCount(0)
    await expectNoRawValues(page)
    expect(api.callsTo("POST", "/workspace/:ws/tax-profiles/:id/verify").at(-1)!.body).toMatchObject({
      expected_version: 1,
      payload: { review_fingerprint: "fp-tax", evidence_ref: "Finance Act 2026" },
    })
  })
})

test("a verified profile is corrected as a new draft revision, prefilled", async ({ page, api }) => {
  const state = taxes(api, [verified()])
  api.on("POST", "/workspace/:ws/tax-profiles", ({ body }) => {
    const p = (body as { payload: Record<string, unknown> }).payload
    const draft = profile({ ...p, id: "tp-3", revision: 2, corrects_id: "tp-1", lineage_id: "tp-1" })
    state.list = [...state.list, draft]
    return { status: 201, body: { data: draft } }
  })
  await page.goto("/settings/taxes/tp-1")
  await page.getByRole("link", { name: "Make a correction" }).click()
  await expect(page.getByRole("heading", { name: "Correct tax profile" })).toBeVisible()
  await expect(page.getByLabel("Rate (%)")).toHaveValue("18.00000000")
  await page.getByLabel("Rate (%)").fill("16")
  await page.getByRole("button", { name: "Save draft" }).click()
  await expect(page).toHaveURL(/\/settings\/taxes\/tp-3/)
  await expect(page.getByRole("heading", { name: /revision 2/ })).toBeVisible()
  await expect(page.getByRole("link", { name: "Open the revision this corrects" })).toBeVisible()
  await expectFits(page)
  await expectNoRawValues(page)
  expect(api.callsTo("POST", "/workspace/:ws/tax-profiles").at(-1)!.body).toMatchObject({
    payload: { corrects_id: "tp-1", components: [{ rate: "16" }] },
  })
})

test("a verified profile's edit page says only drafts change", async ({ page, api }) => {
  taxes(api, [verified()])
  await page.goto("/settings/taxes/tp-1/edit")
  await expect(page.getByText("Only a draft can be changed.")).toBeVisible()
  await expectFits(page)
})
