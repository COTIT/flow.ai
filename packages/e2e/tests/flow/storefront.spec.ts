import { expectNoHorizontalScroll } from "../../src/test"
import {
  designs,
  draft,
  expect,
  loggedOut,
  member,
  previewFrame,
  published,
  revisionButton,
  root,
  test,
} from "./storefront-api"

const region = "Storefront preview"

test("draft and published are marked; the frame shows the live catalog", async ({
  page,
  api,
}) => {
  designs(api)
  await page.goto("/storefront")
  await expect(
    page.getByRole("heading", { level: 1, name: "Storefront design" })
  ).toBeVisible()
  const rows = page.getByRole("row")
  await expect(rows.filter({ hasText: "Draft" })).toHaveCount(1)
  await expect(
    rows.filter({ hasText: "Published" }).filter({ hasNotText: "Previously" })
  ).toHaveCount(1)
  // Ids stay out of accessible names.
  await expect(page.getByRole("button", { name: draft })).toHaveCount(0)
  // The short code is the id's random tail: ids minted in the same minute share their head.
  await expect(rows.filter({ hasText: "Draft" })).toContainText(draft.slice(-8))
  await expect(
    rows.filter({ hasText: "Published" }).filter({ hasNotText: "Previously" })
  ).toContainText(published.slice(-8))

  await page.getByRole("button", { name: revisionButton(2) }).click()
  const preview = page.getByRole("region", { name: region })
  await expect(preview.getByText("Draft — not live yet")).toBeVisible()
  const frame = await previewFrame(page)
  await expect(
    frame.getByRole("heading", { name: "From farm to your table" })
  ).toBeVisible()
  await expect(frame.getByText("Honey jar 1")).toBeVisible()
  await expect(frame.getByText("Few left")).toBeVisible()
  await expect(
    preview.getByRole("button", { name: "Publish", exact: true })
  ).toBeEnabled()
  await expect(
    preview.getByRole("button", { name: "Restore as draft" })
  ).toBeDisabled()
  await expectNoHorizontalScroll(page)

  await page.getByRole("button", { name: revisionButton(1) }).click()
  await expect(preview.getByText("Published — live in your shop")).toBeVisible()
  await expect(
    preview.getByRole("button", { name: "Publish", exact: true })
  ).toBeDisabled()
  await expect(
    preview.getByRole("button", { name: "Restore as draft" })
  ).toBeEnabled()
  expect(api.calls.filter((c) => c.method !== "GET")).toHaveLength(0)
})

test("publishing needs its own confirmation and sends the reviewed version once", async ({
  page,
  api,
}) => {
  designs(api)
  await page.goto("/storefront")
  await page.getByRole("button", { name: revisionButton(2) }).click()
  const preview = page.getByRole("region", { name: region })
  await previewFrame(page)
  await preview.getByRole("button", { name: "Publish", exact: true }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog).toContainText("Publish this draft?")
  await dialog.getByRole("button", { name: "Cancel" }).click()
  await expect(dialog).toBeHidden()
  expect(api.callsTo("POST", `${root()}/publish`)).toHaveLength(0)

  await preview.getByRole("button", { name: "Publish", exact: true }).click()
  await dialog.getByRole("button", { name: "Publish", exact: true }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Storefront published." })
  ).toBeVisible()
  const calls = api.callsTo("POST", `${root()}/publish`)
  expect(calls).toHaveLength(1)
  expect(calls[0]?.body).toEqual({ expected_version: 2, revision_id: draft })
  expect(calls[0]?.headers["idempotency-key"]).toBeTruthy()
  expect(api.callsTo("POST", `${root()}/restore`)).toHaveLength(0)
})

test("restoring history uses its own dialog and never publishes", async ({
  page,
  api,
}) => {
  designs(api)
  await page.goto("/storefront")
  await page.getByRole("button", { name: revisionButton(1) }).click()
  const preview = page.getByRole("region", { name: region })
  await previewFrame(page)
  await preview.getByRole("button", { name: "Restore as draft" }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog).toContainText("Restore this revision as a draft?")
  await dialog.getByRole("button", { name: "Restore as draft" }).click()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Revision restored as a new draft." })
  ).toBeVisible()
  const calls = api.callsTo("POST", `${root()}/restore`)
  expect(calls).toHaveLength(1)
  expect(calls[0]?.body).toEqual({
    expected_version: 2,
    revision_id: published,
  })
  expect(calls[0]?.headers["idempotency-key"]).toBeTruthy()
  expect(api.callsTo("POST", `${root()}/publish`)).toHaveLength(0)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/storefront/pages/home/publish", status: 409 },
      ],
    },
  })

  test("a 409 clears the review and shows the conflict; no automatic retry", async ({
    page,
    api,
  }) => {
    designs(api)
    api.on("POST", `${root()}/publish`, () => ({
      status: 409,
      body: { message: "Storefront design changed." },
    }))
    await page.goto("/storefront")
    await page.getByRole("button", { name: revisionButton(2) }).click()
    await previewFrame(page)
    await page
      .getByRole("region", { name: region })
      .getByRole("button", { name: "Publish", exact: true })
      .click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Publish", exact: true })
      .click()
    await expect(
      page.getByText("The storefront changed.", { exact: false })
    ).toBeVisible()
    await expect(page.getByRole("region", { name: region })).toHaveCount(0)
    await expect(page.getByRole("alertdialog")).toHaveCount(0)
    expect(api.callsTo("POST", `${root()}/publish`)).toHaveLength(1)
  })
})

test("no design yet: an empty list and no preview", async ({ page, api }) => {
  api
    .on("GET", root(), () => ({
      data: {
        version: 0,
        draft_revision_id: null,
        published_revision_id: null,
        draft: null,
      },
    }))
    .on("GET", `${root()}/revisions`, () => ({
      data: [],
      meta: {
        current_page: 1,
        last_page: 1,
        per_page: 50,
        total: 0,
        from: null,
        to: null,
      },
    }))
  await page.goto("/storefront")
  await expect(
    page.getByText("No design drafts yet.", { exact: true })
  ).toBeVisible()
  await expect(page.locator("iframe")).toHaveCount(0)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/storefront/pages/home", status: 500 },
        {
          url: `/v1/workspace/w1/storefront/pages/home/preview?revision_id=${draft}`,
          status: 422,
        },
      ],
    },
  })

  test("a failed load shows an error and retries without writing", async ({
    page,
    api,
  }) => {
    designs(api)
    let fail = true
    api.on("GET", root(), () =>
      fail
        ? { status: 500, body: { message: "Server error." } }
        : {
            data: {
              version: 2,
              draft_revision_id: draft,
              published_revision_id: published,
              draft: null,
            },
          }
    )
    await page.goto("/storefront")
    await expect(page.getByText("Server error.")).toBeVisible()
    fail = false
    await page.getByRole("button", { name: "Try again" }).first().click()
    await expect(page.getByText("Server error.")).toHaveCount(0)
    expect(api.calls.filter((c) => c.method !== "GET")).toHaveLength(0)
  })

  test("a preview the API refuses shows its reason, never a frame", async ({
    page,
    api,
  }) => {
    designs(api)
    api.on("GET", `${root()}/preview`, () => ({
      status: 422,
      body: { message: "This revision no longer passes validation." },
    }))
    await page.goto("/storefront")
    await page.getByRole("button", { name: revisionButton(2) }).click()
    await expect(
      page.getByText("This revision no longer passes validation.")
    ).toBeVisible()
    await expect(page.locator("iframe")).toHaveCount(0)
  })
})

test("members without administration access never retrieve designs", async ({
  page,
  api,
}) => {
  api.on("GET", "/me", () => ({ data: { user: member } }))
  await page.goto("/storefront")
  await expect(
    page.getByText(
      "Only workspace owners and admins can review the storefront.",
      { exact: true }
    )
  ).toBeVisible()
  expect(api.calls.filter((c) => c.path.includes("/storefront/"))).toHaveLength(
    0
  )
  await expect(page.locator("iframe")).toHaveCount(0)
})

test("logged out: /storefront goes to the login page", async ({
  page,
  api,
}) => {
  await loggedOut(page)
  await page.goto("/storefront")
  await expect(page).toHaveURL(/\/login$/)
  expect(api.calls.filter((c) => c.path.includes("/storefront/"))).toHaveLength(
    0
  )
})

test("switching workspace reloads the frame for the other workspace only", async ({
  page,
  api,
}) => {
  designs(api)
  designs(api, "w2", "Ruaha harvest")
  await page.goto("/storefront")
  await page.getByRole("button", { name: revisionButton(2) }).click()
  let frame = await previewFrame(page)
  await expect(
    frame.getByRole("heading", { name: "From farm to your table" })
  ).toBeVisible()

  await page.getByRole("combobox", { name: "Workspace" }).click()
  await page.getByRole("option", { name: "Ruaha Growers" }).click()
  await expect(page.getByRole("region", { name: region })).toHaveCount(0)
  const before = api.callsTo("GET", `${root()}/preview`).length
  await page.getByRole("button", { name: revisionButton(1) }).click()
  frame = await previewFrame(page)
  await expect(
    frame.getByRole("heading", { name: "Ruaha harvest" })
  ).toBeVisible()
  await expect(frame.getByText("From farm to your table")).toHaveCount(0)
  expect(api.callsTo("GET", `${root("w2")}/preview`).length).toBeGreaterThan(0)
  expect(api.callsTo("GET", `${root()}/preview`)).toHaveLength(before)
})
