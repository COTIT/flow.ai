import { APP_KEY, PRODUCT } from "../../mock/fixtures.mjs"
import { expect, test } from "../../src/test"

// React's production log line for a failed server render: it names no detail by design.
const reactServerError =
  "Error: Minified React error #441; visit https://react.dev/errors/441 for the full message or use the non-minified dev environment for full errors and additional helpful warnings."

for (const { why, id } of [
  { why: "revoked (401)", id: PRODUCT.accountsKeyRevoked },
  { why: "short of a scope (403)", id: PRODUCT.accountsKeyNoScope },
]) {
  const path = `/products/${id}`
  test.describe(`the API refuses the shop's key: ${why}`, () => {
    test.use({
      allow: {
        responses: [{ url: path, status: 500 }],
        console: [reactServerError],
      },
    })

    test("the shop shows its break page and never the key or the API's words", async ({
      page,
    }) => {
      const response = await page.goto(path)
      const html = await response!.text()
      for (const leak of [
        APP_KEY,
        "Unauthenticated.",
        "This action is unauthorized.",
      ])
        expect(html.includes(leak), `page carries "${leak}"`).toBe(false)
      const notice = page
        .getByRole("alert")
        .filter({ hasText: "The shop is taking a break" })
      await expect(notice).toBeVisible()
      await expect(
        notice.getByRole("button", { name: "Try again" })
      ).toBeVisible()
      expect((await page.content()).includes(APP_KEY)).toBe(false)
    })
  })
}

test("the shop's key never reaches the browser", async ({ page }) => {
  const seen: string[] = []
  page.on("request", (r) => {
    const auth = r.headers().authorization
    if (auth) seen.push(r.url())
  })
  const response = await page.goto("/")
  expect((await response!.text()).includes(APP_KEY)).toBe(false)
  await expect(page.getByRole("main")).toBeVisible()
  // The browser never sends a bearer: only the shop's server holds the key.
  expect(seen).toEqual([])
  const scripts = await page.evaluate(() =>
    [...document.scripts].map((s) => s.src).filter(Boolean)
  )
  for (const src of scripts) {
    const body = await (await page.request.get(src)).text()
    expect(body.includes(APP_KEY), `${src} carries the key`).toBe(false)
  }
})
