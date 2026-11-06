import { expect, test, urls } from "../../src/stack"
import { ownProduct } from "./shop"

/** A 1×1 PNG. */
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGMAAQAABQABDQottAAAAABJRU5ErkJggg==",
  "base64"
)

// The stack's own file store (Garage, as on staging): the office uploads a photo, the shop shows
// it from a signed link, the shop's CSP lets it load, and nothing on the page fails.
test("a product photo uploaded in the office loads in the shop", async ({
  page,
  request,
}) => {
  const { owner, id, name } = await ownProduct(request)
  const upload = await request.post(`${urls.api}/v1${owner.ws("/files")}`, {
    headers: { Authorization: `Bearer ${owner.token}` },
    multipart: {
      upload: { name: "photo.png", mimeType: "image/png", buffer: PNG },
      document_name: "photo.png",
      public: "0",
    },
  })
  expect(upload.status(), await upload.text()).toBe(201)
  const { data } = (await upload.json()) as { data: { file_id: string } }
  await owner.put(owner.ws(`/products/${id}/images`), {
    images: [{ file_id: data.file_id }],
  })

  const loaded = page.waitForResponse(
    (r) =>
      r.request().resourceType() === "image" && r.url().includes("/genesy-e2e/")
  )
  await page.goto(`${urls.shop}/products/${id}`)

  expect((await loaded).status()).toBe(200)
  const photo = page.getByRole("img", { name })
  await expect(photo).toBeVisible()
  await expect
    .poll(() =>
      photo.evaluate(
        (img: HTMLImageElement) => img.complete && img.naturalWidth
      )
    )
    .toBe(1)
})
