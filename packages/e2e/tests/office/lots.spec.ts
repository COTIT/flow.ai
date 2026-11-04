import { choose, expect, expectFits, type FakeApi, paged, test } from "./fake-api"

const product = { id: "prod-honey", name: "AFM Honey 500g" }
const passport = {
  draft: { title: "Ilundo honey" },
  published: { title: "Ilundo honey" },
  published_at: "2026-09-03T08:00:00Z",
  version: 3,
}

function lotsApi(api: FakeApi) {
  const lots = [
    {
      id: "lot-1",
      product_id: product.id,
      code: "ILU-2026-01",
      public_code: "7K3M9Q2XRT",
      status: "released",
      produced_on: "2026-09-01",
      best_before: "2028-09-01",
      expires_on: null,
      origin: "Ilundo, Tanzania",
      created_at: "2026-09-02T08:00:00Z",
      updated_at: "2026-09-02T08:00:00Z",
    },
  ]
  const events = [
    {
      id: "ev-1",
      kind: "created",
      from_status: null as string | null,
      to_status: "released",
      user_id: null,
      meta: null,
      created_at: "2026-09-02T08:00:00Z",
    },
  ]
  api
    .on("GET", "/workspace/:ws/products", () => paged([product]))
    .on("GET", "/workspace/:ws/lots", ({ query }) =>
      paged(
        lots.filter(
          (l) => !query.get("status") || l.status === query.get("status")
        )
      )
    )
    .on("POST", "/workspace/:ws/lots", ({ body }) => {
      const b = body as Record<string, string>
      if (lots.some((l) => l.code.toLowerCase() === b.code?.toLowerCase()))
        return {
          status: 409,
          body: { message: "A lot with this code already exists." },
        }
      const lot = {
        ...lots[0]!,
        ...b,
        id: `lot-${lots.length + 1}`,
        public_code: "AB12CD34EF",
      }
      lots.push(lot)
      return { status: 201, body: { data: lot } }
    })
    .on("GET", "/workspace/:ws/lots/:lot", ({ params }) => ({
      data: lots.find((l) => l.id === params[1]),
    }))
    .on("PATCH", "/workspace/:ws/lots/:lot", ({ params, body }) => {
      const b = body as Record<string, string>
      if (b.best_before && b.produced_on && b.best_before < b.produced_on)
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: {
              best_before: ["Best before must be on or after produced on."],
            },
          },
        }
      const lot = lots.find((l) => l.id === params[1])!
      if (b.status && b.status !== lot.status)
        events.unshift({
          ...events[0]!,
          id: `ev-${events.length + 1}`,
          kind: "status_changed",
          from_status: lot.status,
          to_status: b.status,
        })
      Object.assign(lot, b, { updated_at: new Date().toISOString() })
      return { data: lot }
    })
    .on("GET", "/workspace/:ws/lots/:lot/events", () => paged(events))
    // The batch page names its product and shows its passport state.
    .on("GET", "/workspace/:ws/products/:id", () => ({ data: product }))
    .on("GET", "/workspace/:ws/lots/:lot/passport", () => ({
      data: passport,
    }))
  return lots
}

test("create a batch: it opens and shows in the list", async ({
  page,
  api,
}) => {
  lotsApi(api)
  await page.goto("/lots")
  await expect(page.getByRole("link", { name: "ILU-2026-01" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "AFM Honey 500g" })).toBeVisible()

  await page.getByRole("button", { name: "New batch" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Batch code").fill("ILU-2026-02")
  await dialog.getByLabel("Produced on").fill("2026-10-01")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(page).toHaveURL(/\/lots\/lot-2\?done=created$/)
  await expect(
    page.getByRole("status").filter({ hasText: "Batch created." })
  ).toBeVisible()
  await expect(page.getByRole("heading", { name: "ILU-2026-02" })).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/lots")[0]?.body).toMatchObject({
    product_id: product.id,
    code: "ILU-2026-02",
    produced_on: "2026-10-01",
    best_before: null,
  })
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/lots", status: 409 }] },
  })

  test("duplicate code: the API's 409 message is shown", async ({
    page,
    api,
  }) => {
    lotsApi(api)
    await page.goto("/lots")
    await page.getByRole("button", { name: "New batch" }).click()
    await page.getByRole("dialog").getByLabel("Batch code").fill("ilu-2026-01")
    await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "A lot with this code already exists." })
    ).toBeVisible()
  })
})

test("status filter goes to the API", async ({ page, api }) => {
  lotsApi(api)
  await page.goto("/lots")
  await choose(page, "Status", "Recalled")
  // A filter that finds nothing says so, and offers the way back.
  await expect(page.getByText("No matches")).toBeVisible()
  await page.getByRole("button", { name: "Clear filters" }).click()
  await expect(page.getByRole("link", { name: "ILU-2026-01" })).toBeVisible()
  expect(api.calls.at(-1)?.path).toMatch(/\/lots$/)
})

test("product filter goes to the API", async ({ page, api }) => {
  lotsApi(api)
  await page.goto("/lots")
  const sent = page.waitForRequest(
    (r) =>
      new URL(r.url()).pathname.endsWith("/lots") &&
      new URL(r.url()).searchParams.get("product_id") === product.id
  )
  await choose(page, "Product", "AFM Honey 500g")
  await sent
  await expect(page).toHaveURL(/product=prod-honey/)
  await expect(page.getByRole("link", { name: "ILU-2026-01" })).toBeVisible()
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/lots/lot-1", status: 422 }] },
  })

  test("edit: 422 field error shows next to the field", async ({
    page,
    api,
  }) => {
    lotsApi(api)
    await page.goto("/lots/lot-1")
    await page.getByLabel("Best before").fill("2026-01-01")
    await page.getByRole("button", { name: "Save" }).click()
    await expect(
      page.getByText("Best before must be on or after produced on.")
    ).toBeVisible()
    await expect(page.getByLabel("Best before")).toHaveAttribute(
      "aria-invalid",
      "true"
    )
  })
})

test("the batch page: product and origin, status, passport and edit link", async ({
  page,
  api,
}) => {
  lotsApi(api)
  await page.goto("/lots/lot-1")
  await expect(page.getByRole("heading", { name: "ILU-2026-01" })).toBeVisible()
  await expect(page.getByText("AFM Honey 500g, Ilundo, Tanzania")).toBeVisible()
  await expect(
    page.getByRole("main").getByRole("link", { name: "Batches" })
  ).toHaveAttribute("href", "/lots")
  const status = page.getByRole("region", { name: "Status" })
  await expect(status.getByText("Released", { exact: true })).toBeVisible()
  const pass = page.getByRole("region", { name: "Passport" })
  await expect(pass.getByText("Published v3")).toBeVisible()
  await expect(pass.getByText("/b/7K3M9Q2XRT")).toBeVisible()
  await expect(
    pass.getByRole("link", { name: "Edit passport" })
  ).toHaveAttribute("href", "/lots/lot-1/passport")
  await expectFits(page)
})

test("a passport never published shows as a draft", async ({ page, api }) => {
  lotsApi(api)
  api.on("GET", "/workspace/:ws/lots/:lot/passport", () => ({
    data: { ...passport, published: null, published_at: null, version: 0 },
  }))
  await page.goto("/lots/lot-1")
  await expect(
    page.getByRole("region", { name: "Passport" }).getByText("Draft", {
      exact: true,
    })
  ).toBeVisible()
})

test("recall asks first; keeping it sends nothing", async ({ page, api }) => {
  lotsApi(api)
  await page.goto("/lots/lot-1")
  await page.getByRole("button", { name: "Recall this batch…" }).click()
  await expect(page.getByRole("alertdialog")).toContainText(
    "Recall batch ILU-2026-01?"
  )
  await page.getByRole("button", { name: "Keep selling" }).click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(api.callsTo("PATCH", "/workspace/:ws/lots/:lot")).toHaveLength(0)
})

test("recall from the status field, confirmed: saved and history shows the change", async ({
  page,
  api,
}) => {
  lotsApi(api)
  await page.goto("/lots/lot-1")
  await page.getByRole("combobox", { name: "Status" }).selectOption("recalled")
  await page.getByRole("button", { name: "Save" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Recall", exact: true })
    .click()
  await expect(page.getByText("Released → Recalled")).toBeVisible()
  expect(
    api.callsTo("PATCH", "/workspace/:ws/lots/:lot")[0]?.body
  ).toMatchObject({
    status: "recalled",
  })
})

test("history says who did it: a name, the system, or a user who left", async ({
  page,
  api,
}) => {
  lotsApi(api)
  const event = (id: string, user: { id: string; name: string | null } | null) => ({
    id,
    kind: "updated",
    from_status: null,
    to_status: null,
    user_id: user?.id ?? null,
    user,
    meta: null,
    created_at: "2026-09-02T08:00:00Z",
  })
  api.on("GET", "/workspace/:ws/lots/:lot/events", () =>
    paged([
      event("ev-a", { id: "u1", name: "Romeo T." }),
      event("ev-b", null),
      event("ev-c", { id: "u9", name: null }),
    ])
  )
  await page.goto("/lots/lot-1")
  const history = page.getByRole("region", { name: "History" })
  // On a phone the column is hidden (OfficeLot's `m-hide`), so read the cells, not the a11y tree.
  const who = (row: number) => history.locator("tbody tr").nth(row).locator("td").nth(2)
  await expect(who(0)).toHaveText("Romeo T.")
  await expect(who(1)).toHaveText("System")
  await expect(who(2)).toHaveText("—")
  if ((page.viewportSize()?.width ?? 0) >= 768)
    await expect(history.getByRole("columnheader", { name: "Who" })).toBeVisible()
  await expectFits(page)
})

// Each field of the new-batch form, a value the API refuses for it, and the label it sits under.
const createFields = [
  { field: "code", label: "Batch code", message: "The code may only hold letters, digits and dashes." },
  { field: "produced_on", label: "Produced on", message: "Produced on may not be in the future." },
  { field: "best_before", label: "Best before", message: "Best before must be on or after produced on." },
]

for (const f of createFields)
  test.describe(() => {
    test.use({ allow: { responses: [{ url: "/v1/workspace/w1/lots", status: 422 }] } })

    test(`/lots new batch: the API's 422 on ${f.field} shows at ${f.label}`, async ({
      page,
      api,
    }) => {
      lotsApi(api)
      api.on("POST", "/workspace/:ws/lots", () => ({
        status: 422,
        body: { message: "The given data was invalid.", errors: { [f.field]: [f.message] } },
      }))
      await page.goto("/lots")
      await page.getByRole("button", { name: "New batch" }).click()
      const dialog = page.getByRole("dialog")
      await dialog.getByLabel("Batch code").fill("ILU 2026/02")
      await dialog.getByRole("button", { name: "Save" }).click()
      await expect(dialog.getByText(f.message), `/lots create ${f.field}`).toBeVisible()
      await expect(dialog.getByLabel(f.label)).toHaveAttribute("aria-invalid", "true")
      await expect(page).toHaveURL(/\/lots$/)
      await expectFits(page)
    })
  })

for (const w of [
  {
    write: "new batch",
    method: "POST",
    path: "/workspace/:ws/lots",
    url: "/v1/workspace/w1/lots",
    route: "/lots",
    run: async (page: import("@playwright/test").Page) => {
      await page.getByRole("button", { name: "New batch" }).click()
      await page.getByRole("dialog").getByLabel("Batch code").fill("ILU-2026-02")
      await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
    },
  },
  {
    write: "status save",
    method: "PATCH",
    path: "/workspace/:ws/lots/:lot",
    url: "/v1/workspace/w1/lots/lot-1",
    route: "/lots/lot-1",
    run: async (page: import("@playwright/test").Page) => {
      await page.getByRole("combobox", { name: "Status" }).selectOption("quarantine")
      await page.getByRole("button", { name: "Save" }).click()
    },
  },
])
  test.describe(() => {
    test.use({ allow: { responses: [{ url: w.url, status: 403 }] } })

    test(`${w.route} ${w.write}: the API's 403 shows; nothing changes`, async ({
      page,
      api,
    }) => {
      const lots = lotsApi(api)
      api.on(w.method, w.path, () => ({
        status: 403,
        body: { message: "This action is unauthorized." },
      }))
      await page.goto(w.route)
      await w.run(page)
      await expect(
        page.getByRole("alert").filter({ hasText: "This action is unauthorized." }),
        `${w.route} ${w.write} 403`
      ).toBeVisible()
      await expect(page).toHaveURL(new RegExp(`${w.route}$`))
      expect(lots).toHaveLength(1)
      expect(lots[0]!.status).toBe("released")
      expect(api.callsTo(w.method, w.path)).toHaveLength(1)
    })
  })

test("/lots/[id] recall: a network failure says so and keeps it on sale; recalling again saves it once", async ({
  page,
  api,
}) => {
  const lots = lotsApi(api)
  let fail = true
  await page.route("**/v1/workspace/w1/lots/lot-1", async (route) => {
    if (route.request().method() === "PATCH" && fail) {
      fail = false
      return route.abort("aborted")
    }
    return route.fallback()
  })
  await page.goto("/lots/lot-1")
  const recall = async () => {
    await page.getByRole("combobox", { name: "Status" }).selectOption("recalled")
    await page.getByRole("button", { name: "Save" }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Recall", exact: true })
      .click()
  }
  await recall()
  await expect(
    page.getByRole("alert").filter({ hasText: "Something went wrong. Try again." })
  ).toBeVisible()
  const status = page.getByRole("region", { name: "Status" })
  await expect(status.getByText("Released", { exact: true })).toBeVisible()
  expect(lots[0]!.status).toBe("released")

  await expect(page.getByRole("alertdialog")).toBeHidden()
  await recall()
  await expect(page.getByText("Released → Recalled")).toBeVisible()
  expect(api.callsTo("PATCH", "/workspace/:ws/lots/:lot")).toHaveLength(1)
  expect(lots[0]!.status).toBe("recalled")
})
