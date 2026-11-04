import { expect, paged, test, type FakeApi } from "./fake-api"

function linksApi(api: FakeApi) {
  const links = [
    {
      id: "link-1",
      direction: "incoming",
      partner: { id: "ws-ruaha", name: "RUAHA Project" },
      fields: ["village", "farmer_count"],
      status: "pending",
      granted_at: "2026-10-30T08:00:00Z",
      accepted_at: null as string | null,
      revoked_at: null as string | null,
    },
    {
      id: "link-2",
      direction: "incoming",
      partner: { id: "ws-old", name: "Old Farm Co" },
      fields: [],
      status: "revoked",
      granted_at: "2026-01-01T08:00:00Z",
      accepted_at: "2026-01-02T08:00:00Z",
      revoked_at: "2026-06-01T08:00:00Z",
    },
  ]
  api
    .on("GET", "/workspace/:ws/supply-links", () => paged(links))
    .on("POST", "/workspace/:ws/supply-links/:id/accept", ({ params }) => {
      const l = links.find((x) => x.id === params[1])!
      Object.assign(l, { status: "live", accepted_at: "2026-11-01T08:00:00Z" })
      return { data: l }
    })
    .on("DELETE", "/workspace/:ws/supply-links/:id", ({ params }) => {
      Object.assign(
        links.find((x) => x.id === params[1])!,
        { status: "revoked" }
      )
      return { status: 204 }
    })
}

test("supply links: allow-list shown; accept asks first and cancel sends nothing", async ({
  page,
  api,
}) => {
  linksApi(api)
  await page.goto("/supply")
  await expect(page).toHaveURL(/\/supply\/links$/)
  const ruaha = page.getByRole("row", { name: /RUAHA Project/ })
  await expect(ruaha.getByText("Village, Number of farmers")).toBeVisible()
  await expect(
    page
      .getByRole("row", { name: /Old Farm Co/ })
      .getByText("No farmer details")
  ).toBeVisible()
  // A revoked link offers nothing.
  await expect(
    page.getByRole("row", { name: /Old Farm Co/ }).getByRole("button")
  ).toHaveCount(0)

  await ruaha.getByRole("button", { name: "Accept" }).click()
  await expect(page.getByRole("alertdialog")).toContainText(
    "Village, Number of farmers"
  )
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Cancel" })
    .click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(
    api.callsTo("POST", "/workspace/:ws/supply-links/:id/accept")
  ).toHaveLength(0)

  await ruaha.getByRole("button", { name: "Accept" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Accept" })
    .click()
  await expect(ruaha.getByText("Live", { exact: true })).toBeVisible()
  await expect(
    page.getByRole("status").filter({ hasText: "Supply link accepted." })
  ).toBeVisible()
  await expect(ruaha.getByRole("button", { name: "Accept" })).toHaveCount(0)
  expect(
    api.callsTo("POST", "/workspace/:ws/supply-links/:id/accept")
  ).toHaveLength(1)
})

test("supply links: revoke asks first; cancel sends nothing", async ({
  page,
  api,
}) => {
  linksApi(api)
  await page.goto("/supply/links")
  const ruaha = page.getByRole("row", { name: /RUAHA Project/ })
  await ruaha.getByRole("button", { name: "Revoke" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Cancel" })
    .click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(api.callsTo("DELETE", "/workspace/:ws/supply-links/:id")).toHaveLength(
    0
  )

  await ruaha.getByRole("button", { name: "Revoke" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Revoke" })
    .click()
  await expect(ruaha.getByText("Revoked", { exact: true })).toBeVisible()
  expect(api.callsTo("DELETE", "/workspace/:ws/supply-links/:id")).toHaveLength(
    1
  )
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/supply-links/link-1/accept", status: 403 },
      ],
    },
  })

  test("supply links: a refused accept shows the API's words", async ({
    page,
    api,
  }) => {
    linksApi(api)
    api.on("POST", "/workspace/:ws/supply-links/:id/accept", () => ({
      status: 403,
      body: { message: "Only workspace admins can do this." },
    }))
    await page.goto("/supply/links")
    await page.getByRole("button", { name: "Accept" }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Accept" })
      .click()
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Only workspace admins can do this." })
    ).toBeVisible()
  })
})

for (const r of [
  { status: 403, message: "Only workspace admins can do this." },
  { status: 409, message: "This link was already revoked by the partner." },
])
  test.describe(() => {
    test.use({
      allow: { responses: [{ url: "/v1/workspace/w1/supply-links/link-1", status: r.status }] },
    })

    test(`/supply/links revoke: the API's ${r.status} shows; the link stays as it was`, async ({
      page,
      api,
    }) => {
      linksApi(api)
      api.on("DELETE", "/workspace/:ws/supply-links/:id", () => ({
        status: r.status,
        body: { message: r.message },
      }))
      await page.goto("/supply/links")
      const ruaha = page.getByRole("row", { name: /RUAHA Project/ })
      await ruaha.getByRole("button", { name: "Revoke" }).click()
      await page.getByRole("alertdialog").getByRole("button", { name: "Revoke" }).click()
      await expect(
        page.getByRole("alert").filter({ hasText: r.message }),
        `/supply/links revoke ${r.status}`
      ).toBeVisible()
      await expect(ruaha.getByText("Revoked", { exact: true })).toHaveCount(0)
      expect(api.callsTo("DELETE", "/workspace/:ws/supply-links/:id")).toHaveLength(1)
    })
  })
