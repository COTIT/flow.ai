import {
  expect,
  expectFits,
  me,
  paged,
  test,
  WS,
  type FakeApi,
} from "./fake-api"

const batch = (over: Record<string, unknown> = {}) => ({
  id: "b1",
  profile: "products",
  filename: "prices.csv",
  expected_rows: 3,
  mapping: { operation: "create", match_key: "barcode" },
  state: "validated",
  version: 4,
  validation_fingerprint: "f".repeat(64),
  counts: {
    staged: 0,
    valid: 2,
    invalid: 1,
    applied: 0,
    failed: 0,
    received: 3,
  },
  allowed_actions: ["validate", "apply"],
  created_at: "2026-11-16T08:00:00Z",
  ...over,
})

const rows = [
  {
    id: "r1",
    row_number: 1,
    external_key: null,
    fields: { name: "Rice", price: "2.50" },
    status: "valid",
    issues: [],
  },
  {
    id: "r2",
    row_number: 2,
    external_key: null,
    fields: { price: "1" },
    status: "invalid",
    issues: [{ code: "validation", message: "The name field is required." }],
  },
  {
    id: "r3",
    row_number: 3,
    external_key: null,
    fields: { name: "Beans", price: "3" },
    status: "valid",
    issues: [],
  },
]

/** A batch that moves validated → applying → completed as the screen polls it. */
function importsApi(api: FakeApi, current = batch()) {
  let reads = 0
  api
    .on("GET", "/workspace/:ws/imports", () => paged([current]))
    .on("POST", "/workspace/:ws/imports", () => ({
      status: 201,
      body: {
        data: batch({
          state: "uploading",
          version: 1,
          validation_fingerprint: null,
        }),
      },
    }))
    .on("PUT", "/workspace/:ws/imports/:id/chunks/:index", () => ({
      data: batch({
        state: "uploading",
        version: 1,
        validation_fingerprint: null,
      }),
    }))
    .on("POST", "/workspace/:ws/imports/:id/validate", () => ({
      data: current,
    }))
    .on("GET", "/workspace/:ws/imports/:id", () => {
      if (current.state === "applying" && ++reads > 1)
        current = batch({
          state: "completed",
          version: 5,
          allowed_actions: [],
          counts: { ...current.counts, valid: 0, applied: 2 },
        })
      return { data: current }
    })
    .on("GET", "/workspace/:ws/imports/:id/rows", ({ query }) => {
      const status = query.get("status")
      const shown =
        current.state === "completed"
          ? rows.map((r) =>
              r.status === "valid" ? { ...r, status: "applied" } : r
            )
          : rows
      return paged(status ? shown.filter((r) => r.status === status) : shown)
    })
    .on("POST", "/workspace/:ws/imports/:id/apply", () => {
      current = batch({ state: "applying", version: 5, allowed_actions: [] })
      return { status: 202, body: { data: current } }
    })
}

const CSV = "Name,Price,Barcode\nRice,2.50,abc\n,1,\nBeans,3,def\n"

test("no imports yet: empty state and one primary action", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/imports", () => paged([]))
  await page.goto("/settings/imports")
  await expect(page.getByRole("heading", { name: "Imports" })).toBeVisible()
  await expect(page.getByText("No imports yet.")).toBeVisible()
  await expect(page.getByRole("link", { name: "New import" })).toBeVisible()
  await expectFits(page)
})

test("pick a file, match columns, check rows, import the ready ones", async ({
  page,
  api,
}) => {
  importsApi(api)
  await page.route("**/api/imports/parse", (route) =>
    route.fulfill({
      json: {
        data: {
          columns: ["Name", "Price", "Barcode"],
          rows: [
            { line: 2, cells: ["Rice", "2.50", "abc"] },
            { line: 3, cells: ["", "1", ""] },
            { line: 4, cells: ["Beans", "3", "def"] },
          ],
        },
      },
    })
  )
  await page.goto("/settings/imports/new")
  await page.getByLabel(/Spreadsheet/).setInputFiles({
    name: "prices.csv",
    mimeType: "text/csv",
    buffer: Buffer.from(CSV),
  })
  await expect(page.getByText("3 rows found in prices.csv.")).toBeVisible()
  await expect(page.getByRole("combobox", { name: "Name" })).toHaveText(/Name/)
  await expectFits(page)
  await page.getByRole("button", { name: "Check rows" }).click()

  await expect(page).toHaveURL(/\/settings\/imports\/b1$/)
  const create = api.callsTo("POST", "/workspace/:ws/imports")[0]!
  expect(create.headers["idempotency-key"]).toBeTruthy()
  expect(create.body).toMatchObject({
    payload: {
      profile: "products",
      filename: "prices.csv",
      expected_rows: 3,
      mapping: { operation: "create", match_key: "barcode" },
    },
  })
  expect(
    (create.body as { payload: { file_hash: string } }).payload.file_hash
  ).toMatch(/^[0-9a-f]{64}$/)
  expect(
    api.callsTo("PUT", "/workspace/:ws/imports/:id/chunks/:index")[0]?.body
  ).toMatchObject({
    expected_version: 1,
    payload: {
      rows: [
        {
          row_number: 1,
          fields: { name: "Rice", price: "2.50", barcode: "abc" },
        },
        { row_number: 2, fields: { price: "1" } },
        { row_number: 3 },
      ],
    },
  })
  expect(
    api.callsTo("POST", "/workspace/:ws/imports/:id/validate")[0]?.body
  ).toMatchObject({ payload: { expected_chunks: 1 } })

  await expect(
    page.getByText("1 rows need a fix and will not be imported.", {
      exact: false,
    })
  ).toBeVisible()
  await expect(page.getByText("The name field is required.")).toBeVisible()
  await page.getByRole("button", { name: "Needs a fix" }).click()
  await expect(page.getByText("Row 1", { exact: true })).toHaveCount(0)
  await expectFits(page)

  await page.getByRole("button", { name: "Import 2 ready rows" }).click()
  await expect
    .poll(() => api.callsTo("POST", "/workspace/:ws/imports/:id/apply").length)
    .toBe(1)
  expect(
    api.callsTo("POST", "/workspace/:ws/imports/:id/apply")[0]?.body
  ).toMatchObject({
    expected_version: 4,
    payload: { validation_fingerprint: "f".repeat(64) },
  })
  await expect(page.getByText("Import finished: 2 rows imported.")).toBeVisible(
    { timeout: 10_000 }
  )
  await expect(page.getByRole("button", { name: /ready rows/ })).toHaveCount(0)
})

test.describe("refusals in words", () => {
  test.use({
    allow: {
      responses: [
        { url: "/api/imports/parse", status: 413 },
        { url: "/v1/workspace/w1/imports/b1/apply", status: 409 },
      ],
    },
  })

  test("a file the parser refuses shows its reason", async ({ page, api }) => {
    void api
    await page.route("**/api/imports/parse", (route) =>
      route.fulfill({
        status: 413,
        json: {
          message:
            "Spreadsheets can have at most 500 rows. Split the file and share each part.",
        },
      })
    )
    await page.goto("/settings/imports/new")
    await page.getByLabel(/Spreadsheet/).setInputFiles({
      name: "big.csv",
      mimeType: "text/csv",
      buffer: Buffer.from("a\nb\n"),
    })
    await expect(
      page.getByText("Spreadsheets can have at most 500 rows.", {
        exact: false,
      })
    ).toBeVisible()
    await expect(page.getByRole("button", { name: "Check rows" })).toHaveCount(
      0
    )
  })

  test("a batch that changed meanwhile asks to check again", async ({
    page,
    api,
  }) => {
    importsApi(api)
    api.on("POST", "/workspace/:ws/imports/:id/apply", () => ({
      status: 409,
      body: {
        message: "This record changed since it was loaded.",
        code: "stale_document",
      },
    }))
    await page.goto("/settings/imports/b1")
    await page.getByRole("button", { name: "Import 2 ready rows" }).click()
    await expect(
      page.getByText("This import changed since you opened it.", {
        exact: false,
      })
    ).toBeVisible()
    await expect(page.getByText("stale_document")).toHaveCount(0)
  })
})

test("without the right to import products, apply is not offered", async ({
  page,
  api,
}) => {
  importsApi(api)
  api.on("GET", "/me", () => ({
    data: {
      user: {
        ...me,
        permissions: [
          {
            workspace: WS,
            permissions: ["import.list", "import.get", "import.apply"],
          },
        ],
      },
    },
  }))
  await page.goto("/settings/imports/b1")
  await expect(
    page
      .getByText("Ready to import", { exact: false })
      .or(page.getByText("2 ready", { exact: false }))
      .first()
  ).toBeVisible()
  await expect(page.getByRole("button", { name: /ready rows/ })).toHaveCount(0)
})
