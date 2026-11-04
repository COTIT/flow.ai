import { expectNoLayoutShift } from "../../src/test"
import {
  expect,
  expectFits,
  type FakeApi,
  me,
  paged,
  test,
  WS,
} from "./fake-api"

const vatGroups = [
  { id: "vg-7", name: "Sales tax", vat_amount: "7.00", default_group: true },
  { id: "vg-0", name: "Exempt", vat_amount: "0.00", default_group: false },
]

type Variant = {
  id: string
  is_default: boolean
  sku: string | null
  price: string
  relationships: {
    option_values: { value: string; display_value: string | null }[]
  }
}

/** A catalog that keeps what the office creates: products, their sizes, and the VAT groups. */
function catalogApi(api: FakeApi, groups: unknown[] = vatGroups) {
  const product = (id: string, name: string, price: string) => ({
    id,
    name,
    product_nr: `PRO-0000${id.length}`,
    reference: null,
    status: "active",
    price,
    price_incl: price,
    labels: [],
    store_only: false,
    last_updated_at: "2026-11-04T08:00:00Z",
  })
  const products = [product("p1", "Beeswax Block", "9.0000")]
  const variants: Record<string, Variant[]> = {
    p1: [
      {
        id: "v-p1",
        is_default: true,
        sku: null,
        price: "9.0000",
        relationships: { option_values: [] },
      },
    ],
  }
  api
    .on("GET", "/workspace/:ws/products", () => paged(products))
    .on("GET", "/workspace/:ws/settings/vat-groups", () => paged(groups))
    .on("POST", "/workspace/:ws/products", ({ body }) => {
      const b = body as { name: string; price: string }
      const errors: Record<string, string[]> = {}
      if (b.name.length > 255)
        errors.name = ["The name may not be greater than 255 characters."]
      if (b.price.startsWith("-"))
        errors.price = ["The price must be at least 0."]
      if (Object.keys(errors).length)
        return {
          status: 422,
          body: { message: "The given data was invalid.", errors },
        }
      const id = `p-new${products.length}`
      products.unshift(product(id, b.name, b.price))
      variants[id] = [
        {
          id: `v-${id}`,
          is_default: true,
          sku: null,
          price: b.price,
          relationships: { option_values: [] },
        },
      ]
      return { status: 201, body: { data: { product_id: id } } }
    })
    .on("GET", "/workspace/:ws/products/:id", ({ params }) => ({
      data: products.find((p) => p.id === params[1]),
    }))
    .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({
      data: [],
    }))
    .on("GET", "/workspace/:ws/products/:id/stock-movements", () => paged([]))
    .on("GET", "/workspace/:ws/products/:id/images", () => ({ data: [] }))
    .on("GET", "/workspace/:ws/files", () => paged([]))
    .on("GET", "/workspace/:ws/products/:id/variants", ({ params }) => ({
      data: variants[params[1]!] ?? [],
    }))
    .on(
      "POST",
      "/workspace/:ws/products/:id/variants/generate",
      ({ params, body }) => {
        const b = body as {
          option_types: { values: { value: string }[] }[]
          combinations: { price: string; sku?: string }[]
        }
        const combo = b.combinations[0]!
        if (combo.price.startsWith("-"))
          return {
            status: 422,
            body: {
              message: "The given data was invalid.",
              errors: {
                "combinations.0.price": ["The price must be at least 0."],
              },
            },
          }
        const value = b.option_types[0]!.values[0]!.value
        const made: Variant = {
          id: `v-${value}`,
          is_default: false,
          sku: combo.sku ?? null,
          price: combo.price,
          relationships: {
            option_values: [
              { value: value.toLowerCase(), display_value: value },
            ],
          },
        }
        variants[params[1]!]!.push(made)
        return { status: 201, body: { data: [made] } }
      }
    )
}

/** `/me` with only these permissions in WS (no owner, no admin). */
function grant(api: FakeApi, permissions: string[]) {
  api.on("GET", "/me", () => ({
    data: { user: { ...me, permissions: [{ workspace: WS, permissions }] } },
  }))
}

test("create a product: it opens on its page and shows in the list", async ({
  page,
  api,
}) => {
  catalogApi(api)
  await page.goto("/products")
  await expect(page.getByRole("link", { name: "Beeswax Block" })).toBeVisible()
  await expectNoLayoutShift(page)

  await page.getByRole("button", { name: "New product" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Name").fill("Ilundo Honey 250g")
  await dialog.getByLabel("Price excl. VAT").fill("6.50")
  await dialog
    .getByLabel("VAT group")
    .selectOption({ label: "Sales tax · 7.00 %" })
  await expectFits(page)
  await dialog.getByRole("button", { name: "Save" }).click()

  await expect(page).toHaveURL(/\/products\/p-new1\?done=created$/)
  await expect(
    page.getByRole("status").filter({ hasText: "Product created." })
  ).toBeVisible()
  await expect(
    page.getByRole("heading", { name: "Ilundo Honey 250g" })
  ).toBeVisible()
  // Only these keys: the API's defaults make it a physical, stock-tracked product
  // (type physical, has_stock true), which farm supply accepts as a raw product.
  expect(api.callsTo("POST", "/workspace/:ws/products")[0]?.body).toEqual({
    name: "Ilundo Honey 250g",
    price: "6.50",
    vat_group_id: "vg-7",
  })

  await page.goto("/products")
  await expect(
    page.getByRole("link", { name: "Ilundo Honey 250g" })
  ).toBeVisible()
})

test("add a size on the product page", async ({ page, api }) => {
  catalogApi(api)
  await page.goto("/products/p1")
  const sizes = page.getByRole("region", { name: "Sizes" })
  await expect(sizes.getByText("Standard")).toBeVisible()

  await sizes.getByRole("button", { name: "Add size" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Size").fill("250 g")
  await dialog.getByLabel("Price excl. VAT").fill("4.20")
  await expectFits(page)
  await dialog.getByRole("button", { name: "Save" }).click()

  await expect(dialog).toBeHidden()
  await expect(
    page.getByRole("status").filter({ hasText: "Size added." })
  ).toBeVisible()
  await expect(sizes.getByText("250 g")).toBeVisible()
  await expect(sizes.getByText("4.20")).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/products/:id/variants/generate")[0]
      ?.body
  ).toEqual({
    option_types: [{ name: "Size", values: [{ value: "250 g" }] }],
    combinations: [{ option_value_ids: ["250 g"], price: "4.20" }],
  })
  await expectFits(page)
})

test.describe("422", () => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/products", status: 422 },
        { url: "/v1/workspace/w1/products/p1/variants/generate", status: 422 },
      ],
    },
  })

  test("create: each field error shows next to its field", async ({
    page,
    api,
  }) => {
    catalogApi(api)
    await page.goto("/products")
    await page.getByRole("button", { name: "New product" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Name").fill("x".repeat(256))
    await dialog.getByLabel("Price excl. VAT").fill("-1")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByLabel("Name")).toHaveAccessibleDescription(
      "The name may not be greater than 255 characters."
    )
    await expect(
      dialog.getByLabel("Price excl. VAT")
    ).toHaveAccessibleDescription("The price must be at least 0.")
    await expect(page).toHaveURL(/\/products$/)
    await expectFits(page)
  })

  test("add size: the price error shows next to the price", async ({
    page,
    api,
  }) => {
    catalogApi(api)
    await page.goto("/products/p1")
    await page.getByRole("button", { name: "Add size" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Size").fill("1 kg")
    await dialog.getByLabel("Price excl. VAT").fill("-1")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(
      dialog.getByLabel("Price excl. VAT")
    ).toHaveAccessibleDescription("The price must be at least 0.")
    await expect(dialog).toBeVisible()
  })
})

test("without product.create there is no New product or Add size", async ({
  page,
  api,
}) => {
  catalogApi(api)
  grant(api, ["product.list", "product.view", "product.update"])
  const meRead = page.waitForResponse((r) => r.url().endsWith("/v1/me"))
  await page.goto("/products?new=1")
  await meRead
  await expect(page.getByRole("link", { name: "Beeswax Block" })).toBeVisible()
  await expect(page.getByRole("button", { name: "New product" })).toHaveCount(0)
  await expect(page.getByRole("dialog")).toHaveCount(0)

  await page.getByRole("link", { name: "Beeswax Block" }).click()
  const sizes = page.getByRole("region", { name: "Sizes" })
  await expect(sizes.getByText("Standard")).toBeVisible()
  await expect(page.getByRole("button", { name: "Add size" })).toHaveCount(0)
  // Editing stays as it was.
  await expect(page.getByRole("button", { name: "Save changes" })).toBeVisible()
  await expectFits(page)
})

test.describe("refused by the API", () => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/products", status: 403 }] },
  })

  test("a 403 on save shows the API's message in the dialog", async ({
    page,
    api,
  }) => {
    catalogApi(api)
    api.on("POST", "/workspace/:ws/products", () => ({
      status: 403,
      body: { message: "This action is unauthorized." },
    }))
    await page.goto("/products")
    await page.getByRole("button", { name: "New product" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Name").fill("Comb honey")
    await dialog.getByLabel("Price excl. VAT").fill("12")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(
      dialog
        .getByRole("alert")
        .filter({ hasText: "This action is unauthorized." })
    ).toBeVisible()
  })
})

test("no VAT groups: the workspace default applies and nothing is sent", async ({
  page,
  api,
}) => {
  catalogApi(api, [])
  await page.goto("/products?new=1")
  const dialog = page.getByRole("dialog")
  await expect(
    dialog.getByText("No VAT groups yet, so the workspace default applies.")
  ).toBeVisible()
  await expect(dialog.getByLabel("VAT group").locator("option")).toHaveText([
    "Workspace default",
  ])
  await dialog.getByLabel("Name").fill("Comb honey")
  await dialog.getByLabel("Price excl. VAT").fill("12")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(page).toHaveURL(/\/products\/p-new1\?done=created$/)
  expect(api.callsTo("POST", "/workspace/:ws/products")[0]?.body).toEqual({
    name: "Comb honey",
    price: "12",
  })
})

test.describe("VAT groups refused", () => {
  test.use({
    allow: {
      responses: [
        {
          url: "/v1/workspace/w1/settings/vat-groups?page_size=100",
          status: 403,
        },
      ],
    },
  })

  test("the picker keeps the workspace default and says why", async ({
    page,
    api,
  }) => {
    catalogApi(api)
    api.on("GET", "/workspace/:ws/settings/vat-groups", () => ({
      status: 403,
      body: { message: "This action is unauthorized." },
    }))
    await page.goto("/products")
    await page.getByRole("button", { name: "New product" }).click()
    const dialog = page.getByRole("dialog")
    await expect(
      dialog.getByText(
        "VAT groups could not be loaded, so the workspace default applies."
      )
    ).toBeVisible({ timeout: 15_000 })
    await expect(dialog.getByLabel("VAT group").locator("option")).toHaveText([
      "Workspace default",
    ])
  })
})
