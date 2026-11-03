import type { Page } from "@playwright/test"

import { expectNoHorizontalScroll, expectTapTargets } from "../../src/test"
import { expect, paged, test, type FakeApi } from "./fake-api"

const isPhone = (page: Page) => (page.viewportSize()?.width ?? 0) < 768
const previewName = "Live preview · what customers see"

const lot = {
  id: "lot-1",
  product_id: "prod-honey",
  code: "ILU-2026-01",
  public_code: "7K3M9Q2XRT",
  status: "released",
  produced_on: "2026-09-01",
  best_before: "2028-09-01",
  expires_on: null,
  origin: null,
  created_at: "2026-09-02T08:00:00Z",
  updated_at: "2026-09-02T08:00:00Z",
}

function passportApi(api: FakeApi, draft: Record<string, unknown> = {}) {
  const state: {
    draft: Record<string, unknown>
    published: Record<string, unknown> | null
    published_at: string | null
    version: number
  } = {
    draft,
    published: null,
    published_at: null,
    version: 0,
  }
  api
    .on("GET", "/workspace/:ws/lots/:lot", () => ({ data: lot }))
    .on("GET", "/workspace/:ws/products/:id", () => ({
      data: {
        id: lot.product_id,
        name: "AFM Honey 500g",
        brand: "Ilundo",
        short_description: null,
      },
    }))
    .on("GET", "/workspace/:ws/files", () => paged([]))
    .on("GET", "/workspace/:ws/lots/:lot/passport", () => ({ data: state }))
    .on("PUT", "/workspace/:ws/lots/:lot/passport", ({ body }) => {
      state.draft = body as Record<string, unknown>
      return { data: state }
    })
    .on("POST", "/workspace/:ws/lots/:lot/passport/publish", () => {
      if (!state.draft.title)
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: { title: ["A title is required to publish."] },
          },
        }
      state.published = state.draft
      state.version += 1
      state.published_at = "2026-10-31T10:00:00Z"
      return { data: state }
    })
    .on("DELETE", "/workspace/:ws/lots/:lot/passport/publish", () => {
      state.published = null
      return { data: state }
    })
  return state
}

test("edit, save and see it in the preview; shares go out as basis points", async ({
  page,
  api,
}) => {
  passportApi(api)
  await page.goto("/lots/lot-1/passport")
  await page.getByLabel("Title").fill("Wild forest honey")
  await page
    .getByLabel("Introduction")
    .fill("Harvested by the Ilundo beekeepers.")
  await page.getByRole("button", { name: "Add ingredient" }).click()
  const row = page.getByRole("group", { name: "Ingredients 1" })
  await row.getByLabel("Name").fill("Honey")
  await row.getByLabel("Share (%)").fill("62.5")

  // Phones show one side at a time.
  if (isPhone(page)) await page.getByRole("tab", { name: "Preview" }).click()
  const preview = page.getByRole("region", { name: previewName })
  await expect(
    preview.getByRole("heading", { name: "Wild forest honey" })
  ).toBeVisible()
  await expect(
    preview.getByText("Harvested by the Ilundo beekeepers.")
  ).toBeVisible()

  await expect(page.getByRole("button", { name: "Publish v1" })).toBeDisabled()
  await page.getByRole("button", { name: "Save draft" }).click()
  await expect(page.getByRole("button", { name: "Publish v1" })).toBeEnabled()
  expect(
    api.callsTo("PUT", "/workspace/:ws/lots/:lot/passport")[0]?.body
  ).toMatchObject({
    title: "Wild forest honey",
    materials: [{ name: "Honey", share_bp: 6250 }],
  })
})

test("story blocks: add, reorder and save in that order", async ({
  page,
  api,
}) => {
  passportApi(api, { title: "T" })
  await page.goto("/lots/lot-1/passport")
  await page.getByRole("button", { name: "Text", exact: true }).click()
  await page
    .getByRole("group", { name: "Story 1" })
    .getByLabel("Text")
    .fill("First")
  await page.getByRole("button", { name: "Quote", exact: true }).click()
  await page
    .getByRole("group", { name: "Story 2" })
    .getByLabel("Quote")
    .fill("Bees first.")
  await page
    .getByRole("group", { name: "Story 2" })
    .getByLabel("Quote")
    .evaluate((el) => (el.dataset.probe = "moved"))
  await page
    .getByRole("group", { name: "Story 2" })
    .getByRole("button", { name: "Move up" })
    .click()
  // Stable row keys: the moved row keeps its own input element.
  await expect(
    page.getByRole("group", { name: "Story 1" }).locator("[data-probe=moved]")
  ).toHaveValue("Bees first.")
  await page.getByRole("button", { name: "Save draft" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Draft saved." })
  ).toBeVisible()
  expect(
    api.callsTo("PUT", "/workspace/:ws/lots/:lot/passport")[0]?.body
  ).toMatchObject({
    story: [
      { kind: "quote", text: "Bees first." },
      { kind: "text", body: "First" },
    ],
  })
})

test("publish: cancel sends nothing; confirm publishes version 1", async ({
  page,
  api,
}) => {
  passportApi(api, { title: "Wild forest honey" })
  await page.goto("/lots/lot-1/passport")
  await expect(page.getByText("Not published", { exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Publish v1" }).click()
  await expect(page.getByRole("alertdialog")).toContainText("7K3M9Q2XRT")
  await page.getByRole("button", { name: "Cancel" }).click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(
    api.callsTo("POST", "/workspace/:ws/lots/:lot/passport/publish")
  ).toHaveLength(0)

  await page.getByRole("button", { name: "Publish v1" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Publish" })
    .click()
  await expect(page.getByText("Published v1", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Publish v2" })).toBeVisible()
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/lots/lot-1/passport/publish", status: 422 },
      ],
    },
  })

  test("publish refused: the API's 422 shows at the title", async ({
    page,
    api,
  }) => {
    passportApi(api)
    await page.goto("/lots/lot-1/passport")
    await page.getByRole("button", { name: "Publish v1" }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Publish" })
      .click()
    await expect(
      page.getByText("A title is required to publish.")
    ).toBeVisible()
    await expect(page.getByLabel("Title")).toHaveAttribute(
      "aria-invalid",
      "true"
    )
  })
})

test("unpublish: cancel sends nothing; confirm takes it offline", async ({
  page,
  api,
}) => {
  const state = passportApi(api, { title: "Wild forest honey" })
  Object.assign(state, {
    published: state.draft,
    version: 1,
    published_at: "2026-10-31T10:00:00Z",
  })
  await page.goto("/lots/lot-1/passport")
  await page.getByRole("button", { name: "Unpublish" }).click()
  await page.getByRole("button", { name: "Cancel" }).click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(
    api.callsTo("DELETE", "/workspace/:ws/lots/:lot/passport/publish")
  ).toHaveLength(0)

  await page.getByRole("button", { name: "Unpublish" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Unpublish" })
    .click()
  await expect(page.getByRole("button", { name: "Unpublish" })).toHaveCount(0)
  expect(
    api.callsTo("DELETE", "/workspace/:ws/lots/:lot/passport/publish")
  ).toHaveLength(1)
})

test("a draft that differs from the published one says customers still see that version", async ({
  page,
  api,
}) => {
  const state = passportApi(api, { title: "New title" })
  Object.assign(state, {
    published: { title: "Wild forest honey" },
    version: 3,
    published_at: "2026-10-31T10:00:00Z",
  })
  await page.goto("/lots/lot-1/passport")
  await expect(page.getByText("Published v3", { exact: true })).toBeVisible()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Draft differs from the published passport" })
  ).toContainText("Customers still see v3 until you publish.")
  await expect(page.getByRole("button", { name: "Publish v4" })).toBeEnabled()
})

test("phone: Edit and Preview tabs switch between form and preview; desktop shows both and no tabs", async ({
  page,
  api,
}) => {
  passportApi(api, { title: "Wild forest honey" })
  await page.goto("/lots/lot-1/passport")
  const form = page.getByLabel("Title")
  const preview = page.getByRole("region", { name: previewName })
  const heading = preview.getByRole("heading", { name: "Wild forest honey" })

  if (isPhone(page)) {
    await expect(page.getByRole("tab")).toHaveText(["Edit", "Preview"])
    await expect(page.getByRole("tab", { name: "Edit" })).toHaveAttribute(
      "aria-selected",
      "true"
    )
    await expect(form).toBeVisible()
    await expect(preview).toBeHidden()

    await page.getByRole("tab", { name: "Preview" }).click()
    await expect(heading).toBeVisible()
    await expect(form).toBeHidden()

    await page.getByRole("tab", { name: "Edit" }).click()
    await expect(form).toBeVisible()
    await expect(preview).toBeHidden()
    await expectNoHorizontalScroll(page)
    await expectTapTargets(page)
  } else {
    await expect(page.getByRole("tab")).toHaveCount(0)
    await expect(form).toBeVisible()
    await expect(heading).toBeVisible()
  }
})
