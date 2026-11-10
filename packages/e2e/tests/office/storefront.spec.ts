import document from "../../../../apps/api/tests/fixtures/storefront-design-v1.json" with { type: "json" }
import {
  choose,
  expect,
  expectFits,
  me,
  openMenu,
  paged,
  test,
  type FakeApi,
} from "./fake-api"

const root = "/workspace/:ws/storefront/pages/home"
const draft = "01950000-0000-7000-8000-000000000002"
const published = "01950000-0000-7000-8000-000000000001"
const restored = "01950000-0000-7000-8000-000000000003"
// Screen readers hear the revision number and save date, never the UUID.
const draftButton = /^Preview revision 2, saved /
const publishedButton = /^Preview revision 1, saved /

function designs(api: FakeApi) {
  let version = 2
  let currentDraft = draft
  let currentPublished = published
  const state = () => ({
    data: {
      version,
      draft_revision_id: currentDraft,
      published_revision_id: currentPublished,
      draft: { revision_id: currentDraft, document },
    },
  })
  api
    .on("GET", root, state)
    .on("GET", `${root}/revisions`, () =>
      paged(
        [draft, published].map((revision_id) => ({
          revision_id,
          created_at: "2026-11-09T08:00:00Z",
          created_by: "u1",
          restored_from_revision_id: null,
          was_published: revision_id === published,
        }))
      )
    )
    .on("GET", `${root}/preview`, ({ query }) => ({
      data: {
        version,
        revision_id: query.get("revision_id"),
        document,
        collections: [{ section_id: "featured-products", products: [] }],
      },
    }))
    .on("POST", `${root}/publish`, ({ body }) => {
      currentPublished = (body as { revision_id: string }).revision_id
      version++
      return state()
    })
    .on("POST", `${root}/restore`, () => {
      currentDraft = restored
      version++
      return { status: 201, body: state() }
    })
  return state
}

test("storefront preview is read-only; publish requires confirmation and exact reviewed version", async ({
  page,
  api,
}, testInfo) => {
  designs(api)
  await page.goto("/settings/storefront")
  await expect(page.getByRole("button", { name: draft })).toHaveCount(0)
  await expect(page.getByRole("button", { name: published })).toHaveCount(0)
  // The short code is the id's random tail: ids minted in the same minute share their head.
  const rows = page.getByRole("row")
  await expect(rows.filter({ hasText: draft.slice(-8) })).toHaveCount(1)
  await expect(rows.filter({ hasText: published.slice(-8) })).toHaveCount(1)
  await page.getByRole("button", { name: draftButton }).click()
  const preview = page.getByRole("region", { name: "Storefront preview" })
  await expect(
    preview.getByText("From farm to your table", { exact: true })
  ).toBeVisible()
  await expect(preview.getByRole("link")).toHaveCount(0)
  await expectFits(page)
  const screenshot = testInfo.outputPath("storefront-preview.png")
  await page.screenshot({ path: screenshot, fullPage: true })
  await testInfo.attach("storefront-preview", {
    path: screenshot,
    contentType: "image/png",
  })
  await preview.getByRole("button", { name: "Publish", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Cancel" })
    .click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(api.callsTo("POST", `${root}/publish`)).toHaveLength(0)
  await preview.getByRole("button", { name: "Publish", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Publish", exact: true })
    .click()
  await expect(
    page.getByRole("status").filter({ hasText: "Storefront published." })
  ).toBeVisible()
  const calls = api.callsTo("POST", `${root}/publish`)
  expect(calls).toHaveLength(1)
  expect(calls[0]?.body).toEqual({ expected_version: 2, revision_id: draft })
  expect(calls[0]?.headers["idempotency-key"]).toBeTruthy()
})

test("restoring history creates a draft and never publishes it", async ({
  page,
  api,
}) => {
  designs(api)
  await page.goto("/settings/storefront")
  await page.getByRole("button", { name: publishedButton }).click()
  await expect(
    page.getByRole("region", { name: "Storefront preview" })
  ).toContainText("From farm to your table")
  await expect(
    page.getByRole("button", { name: "Publish", exact: true })
  ).toBeDisabled()
  await page.getByRole("button", { name: "Restore as draft" }).click()
  await expect(page.getByRole("alertdialog")).toContainText(
    "Restore this revision as a draft?"
  )
  // Records any moment the publish wording shows while Restore closes.
  await page.evaluate(() => {
    const w = window as unknown as { flipped: boolean }
    w.flipped = false
    new MutationObserver(() => {
      const dialogs = window.document.querySelectorAll('[role="alertdialog"]')
      if (
        [...dialogs].some((d) => d.textContent?.includes("Publish this draft?"))
      )
        w.flipped = true
    }).observe(window.document.body, {
      subtree: true,
      childList: true,
      characterData: true,
    })
  })
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Cancel" })
    .click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(
    await page.evaluate(
      () => (window as unknown as { flipped: boolean }).flipped
    )
  ).toBe(false)
  expect(api.callsTo("POST", `${root}/restore`)).toHaveLength(0)
  await page.getByRole("button", { name: "Restore as draft" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Restore as draft" })
    .click()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Revision restored as a new draft." })
  ).toBeVisible()
  expect(api.callsTo("POST", `${root}/restore`)[0]?.body).toEqual({
    expected_version: 2,
    revision_id: published,
  })
  expect(api.callsTo("POST", `${root}/publish`)).toHaveLength(0)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/storefront/pages/home/publish", status: 409 },
      ],
    },
  })
  test("conflict clears review; no automatic publish retry", async ({
    page,
    api,
  }) => {
    designs(api)
    api.on("POST", `${root}/publish`, () => ({
      status: 409,
      body: {
        message: "This storefront changed. Refresh and review it again.",
      },
    }))
    await page.goto("/settings/storefront")
    await page.getByRole("button", { name: draftButton }).click()
    await expect(
      page.getByRole("button", { name: "Publish", exact: true })
    ).toBeEnabled()
    await page.getByRole("button", { name: "Publish", exact: true }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Publish", exact: true })
      .click()
    await expect(
      page.getByRole("status").filter({
        hasText:
          "The storefront changed. Select a revision to refresh its preview and review it again.",
      })
    ).toBeVisible()
    await expect(
      page.getByText("This storefront changed. Refresh and review it again.")
    ).toHaveCount(0)
    await expect(
      page.getByRole("region", { name: "Storefront preview" })
    ).toHaveCount(0)
    expect(api.callsTo("POST", `${root}/publish`)).toHaveLength(1)
  })
})

test("empty storefront keeps existing shop live", async ({ page, api }) => {
  api.on("GET", root, () => ({
    data: {
      version: 0,
      draft_revision_id: null,
      published_revision_id: null,
      draft: null,
    },
  }))
  api.on("GET", `${root}/revisions`, () => paged([]))
  await page.goto("/settings/storefront")
  await expect(
    page.getByText("No design drafts yet.", { exact: true })
  ).toBeVisible()
  expect(api.callsTo("GET", `${root}/preview`)).toHaveLength(0)
  await expectFits(page)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/storefront/pages/home", status: 503 },
      ],
    },
  })

  test("state load failure shows API error and retries without a write", async ({
    page,
    api,
  }) => {
    const state = designs(api)
    api.on("GET", root, () => ({
      status: 503,
      body: { message: "Design storage is unavailable." },
    }))
    await page.goto("/settings/storefront")
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Design storage is unavailable." })
    ).toBeVisible()
    api.on("GET", root, state)
    await page.getByRole("button", { name: "Try again" }).click()
    await expect(page.getByRole("button", { name: draftButton })).toBeEnabled()
    expect(api.callsTo("POST", `${root}/publish`)).toHaveLength(0)
    expect(api.callsTo("POST", `${root}/restore`)).toHaveLength(0)
  })
})

test("members without administration access never retrieve designs", async ({
  page,
  api,
}) => {
  api.on("GET", "/me", () => ({
    data: {
      user: {
        ...me,
        permissions: [{ workspace: "w1", permissions: ["product.list"] }],
      },
    },
  }))
  await page.goto("/settings/storefront")
  await expect(
    page.getByText("You don't have access to this", { exact: true })
  ).toBeVisible()
  expect(api.callsTo("GET", root)).toHaveLength(0)
  expect(api.callsTo("GET", `${root}/revisions`)).toHaveLength(0)
})

test("workspace switch clears selected preview", async ({ page, api }) => {
  designs(api)
  api.on("GET", "/workspace/w2/storefront/pages/home", () => ({
    data: {
      version: 0,
      draft_revision_id: null,
      published_revision_id: null,
      draft: null,
    },
  }))
  api.on("GET", "/workspace/w2/storefront/pages/home/revisions", () =>
    paged([])
  )
  await page.goto("/settings/storefront")
  await page.getByRole("button", { name: draftButton }).click()
  await expect(
    page.getByRole("region", { name: "Storefront preview" })
  ).toBeVisible()
  await openMenu(page)
  await choose(page, "Workspace", "Other Co")
  await expect(
    page.getByText("No design drafts yet.", { exact: true })
  ).toBeVisible()
  await expect(
    page.getByRole("region", { name: "Storefront preview" })
  ).toHaveCount(0)
  await expect(page.getByRole("alertdialog")).toHaveCount(0)
  expect(api.callsTo("POST", `${root}/publish`)).toHaveLength(0)
})
