import { FILES_ORIGIN, LOT, PRODUCT } from "../../mock/fixtures.mjs"
import { expect, test } from "../../src/test"

// Product photos come as signed links on the file host's own origin (Backblaze B2 in
// production). The CSP lets exactly that origin in (FILES_ORIGIN); the page guard fails any
// test that logs a CSP refusal or a failed image request.
for (const path of [
  "/",
  "/shop",
  `/products/${PRODUCT.honey}`,
  `/b/${LOT.released}`,
])
  test(`${path}: photos from the file host load under the CSP`, async ({
    page,
  }) => {
    const response = await page.goto(path)
    const csp = response!.headers()["content-security-policy"]!
    expect(csp).toMatch(new RegExp(`img-src [^;]*${FILES_ORIGIN}`))

    const photos = page.locator(`img[src^="${FILES_ORIGIN}/files/"]`)
    await expect(photos.first()).toBeVisible()
    // By index, so a photo re-mounted after hydration is found again.
    for (let i = 0, n = await photos.count(); i < n; i++) {
      const img = photos.nth(i)
      await expect
        .poll(() =>
          img.evaluate((el: HTMLImageElement) => el.complete && el.naturalWidth)
        )
        .toBeGreaterThan(0)
      // Signed links don't leak the page address to the file host.
      await expect(img).toHaveAttribute("referrerpolicy", "no-referrer")
    }
  })
