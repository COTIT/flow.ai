import { expect, paged, test, type FakeApi } from "./fake-api"

// Data URLs keep the thumbnails inside the office CSP in tests (no storage host here).
const pixel = "data:image/gif;base64,R0lGODlhAQABAAAAACw="
const files = [
  { id: "f-jar", document_name: "jar-front.jpg", file_path: pixel },
  { id: "f-label", document_name: "label.jpg", file_path: pixel },
]

function imagesApi(
  api: FakeApi,
  start: { file_id: string; product_variant_id: string | null }[]
) {
  let images = start.map((img, i) => ({
    ...img,
    position: i + 1,
    link: { url: pixel, expires_at: "2099-01-01T00:00:00Z" },
  }))
  api
    .on("GET", "/workspace/:ws/products/:id", () => ({
      data: {
        id: "p1",
        name: "AFM Honey 500g",
        store_only: false,
        labels: [],
        last_updated_at: "x",
      },
    }))
    .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({
      data: [],
    }))
    .on("GET", "/workspace/:ws/products/:id/stock-movements", () => paged([]))
    .on("GET", "/workspace/:ws/files", () => paged(files))
    .on("GET", "/workspace/:ws/products/:id/images", () => ({ data: images }))
    .on("PUT", "/workspace/:ws/products/:id/images", ({ body }) => {
      const b = body as {
        images: { file_id: string; product_variant_id: string | null }[]
      }
      if (b.images.some((x) => x.file_id === "f-label"))
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: {
              [`images.${b.images.findIndex((x) => x.file_id === "f-label")}.file_id`]:
                ["The file is not an image."],
            },
          },
        }
      images = b.images.map((img, i) => ({
        ...img,
        position: i + 1,
        link: { url: pixel, expires_at: "2099-01-01T00:00:00Z" },
      }))
      return { data: images }
    })
}

test("no images yet; add one and save the whole list", async ({
  page,
  api,
}) => {
  imagesApi(api, [])
  await page.goto("/products/p1")
  await expect(page.getByText("No images yet.")).toBeVisible()
  await page.getByRole("button", { name: "Add image" }).click()
  await page.getByRole("dialog").getByLabel("Image file").selectOption("f-jar")
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Add image" })
    .click()
  await expect(page.getByText("1. jar-front.jpg")).toBeVisible()
  await page.getByRole("button", { name: "Save images" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Images saved." })
  ).toBeVisible()
  expect(
    api.callsTo("PUT", "/workspace/:ws/products/:id/images")[0]?.body
  ).toEqual({
    images: [{ file_id: "f-jar", product_variant_id: null }],
  })
  await expect(page.getByRole("button", { name: "Save images" })).toHaveCount(0)
})

test("reorder and remove keep variant images in the list sent", async ({
  page,
  api,
}) => {
  imagesApi(api, [
    { file_id: "f-jar", product_variant_id: null },
    { file_id: "f-back", product_variant_id: "v-1kg" },
    { file_id: "f-side", product_variant_id: null },
  ])
  await page.goto("/products/p1")
  await expect(page.getByText("variant image")).toBeVisible()
  await page.getByRole("button", { name: "Remove image 3" }).click()
  await page.getByRole("button", { name: "Move up" }).nth(1).click()
  await page.getByRole("button", { name: "Save images" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Images saved." })
  ).toBeVisible()
  expect(
    api.callsTo("PUT", "/workspace/:ws/products/:id/images")[0]?.body
  ).toEqual({
    images: [
      { file_id: "f-back", product_variant_id: "v-1kg" },
      { file_id: "f-jar", product_variant_id: null },
    ],
  })
})

test("discard puts the saved list back without sending anything", async ({
  page,
  api,
}) => {
  imagesApi(api, [{ file_id: "f-jar", product_variant_id: null }])
  await page.goto("/products/p1")
  await page.getByRole("button", { name: "Remove image 1" }).click()
  await expect(page.getByText("No images yet.")).toBeVisible()
  await page.getByRole("button", { name: "Discard changes" }).click()
  await expect(page.getByText("1. jar-front.jpg")).toBeVisible()
  expect(api.callsTo("PUT", "/workspace/:ws/products/:id/images")).toHaveLength(
    0
  )
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/products/p1/images", status: 422 }],
    },
  })

  test("a refused file shows the API's words on its row", async ({
    page,
    api,
  }) => {
    imagesApi(api, [])
    await page.goto("/products/p1")
    await page.getByRole("button", { name: "Add image" }).click()
    await page.getByRole("dialog").getByLabel("Image file").selectOption("f-label")
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Add image" })
      .click()
    await page.getByRole("button", { name: "Save images" }).click()
    await expect(page.getByText("The file is not an image.")).toBeVisible()
    await expect(
      page.getByRole("status").filter({ hasText: "Images saved." })
    ).toHaveCount(0)
  })
})
