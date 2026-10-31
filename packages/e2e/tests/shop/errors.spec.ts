import { LOT, PRODUCT, SECRET_BODY } from "../../mock/fixtures.mjs"
import { expect, test } from "../../src/test"

// React's production log line for a failed server render: it names no detail by design.
const reactServerError =
  "Error: Minified React error #441; visit https://react.dev/errors/441 for the full message or use the non-minified dev environment for full errors and additional helpful warnings."

// A stack frame ("at fn (file.ts:12:3)") or a source path in the page would leak internals.
const stack = /\bat \S+ \(|\.tsx?:\d+/

for (const path of [`/products/${PRODUCT.failing}`, `/b/${LOT.failing}`]) {
  test.describe(`API down for ${path}`, () => {
    test.use({
      allow: {
        responses: [{ url: path, status: 500 }],
        console: [reactServerError],
      },
    })

    test("shows the friendly error page and nothing from upstream", async ({
      page,
    }) => {
      const response = await page.goto(path)
      expect(response?.status()).toBe(500)
      const html = await response!.text()
      expect(html).not.toContain(SECRET_BODY)
      expect(html).not.toMatch(stack)

      await expect(
        page.getByRole("heading", { name: "The shop is taking a break" })
      ).toBeVisible()
      await expect(
        page.getByRole("button", { name: "Try again" })
      ).toBeVisible()
      expect(await page.content()).not.toContain(SECRET_BODY)
    })
  })
}
