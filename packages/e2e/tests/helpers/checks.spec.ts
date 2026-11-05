import {
  expect,
  expectNoLayoutShift,
  expectNoMotion,
  expectNoRawValues,
  expectTapTargets,
  test,
} from "../../src/test"

// The shared checks themselves, on tiny pages: each must pass a good page and catch a bad one.

test("tap targets: passes 44 px controls, names the small ones", async ({ page }) => {
  await page.setContent(
    `<button style="width:48px;height:48px">Big</button><a href="#" style="display:inline-block;width:60px;height:44px">Link</a>`
  )
  await expectTapTargets(page)

  await page.setContent(
    `<button style="width:48px;height:48px">Big</button><button style="width:30px;height:30px">Tiny</button>`
  )
  await expect(expectTapTargets(page)).rejects.toThrow(/Tiny/)
})

test("tap targets: skips controls hidden from users", async ({ page }) => {
  await page.setContent(
    `<button style="width:48px;height:48px">Pick</button><input aria-hidden="true" tabindex="-1" style="width:1px;height:1px">`
  )
  await expectTapTargets(page)
})

test("tap targets: counts a hit area widened by ::after", async ({ page }) => {
  await page.setContent(
    `<style>.hit{position:relative;width:32px;height:18px}.hit::after{content:"";position:absolute;inset:-13px -6px}</style><button class="hit">On</button>`
  )
  await expectTapTargets(page)
  await page.setContent(`<button style="width:32px;height:18px">Off</button>`)
  await expect(expectTapTargets(page)).rejects.toThrow(/Off/)
})

test("tap targets: can be limited to part of the page", async ({ page }) => {
  await page.setContent(
    `<nav><button style="width:20px;height:20px">x</button></nav><main><button style="width:64px;height:64px">Pay</button></main>`
  )
  await expectTapTargets(page, { within: "main" })
})

test("layout shift: passes a still page, catches content pushed down", async ({ page }) => {
  await page.setContent(`<p style="height:40px">Top</p><p>Rest of the page</p>`)
  await page.waitForTimeout(300)
  await expectNoLayoutShift(page)

  await page.setContent(
    `<div id="slot"></div><p style="height:200px">Text</p><p>More</p>
     <script>setTimeout(() => { document.getElementById("slot").style.height = "300px" }, 300)</script>`
  )
  await page.waitForTimeout(800)
  // A shift only counts after the first paint, so the push comes a moment later.
  await expect(expectNoLayoutShift(page)).rejects.toThrow(/layout shift/)
})

test("motion: none rendered passes, an animated island fails", async ({ page }) => {
  await page.setContent(`<p>Still</p>`)
  await expectNoMotion(page)

  await page.setContent(`<span data-motion style="display:contents"><b>12</b></span>`)
  await expect(expectNoMotion(page)).rejects.toThrow()
})

test("raw values: passes readable text, keys, URLs and allowed codes", async ({ page }) => {
  await page.setContent(
    `<h1>App keys</h1><p>Last used 4 Nov 2026, 18:12. Note: read the docs at https://genesy.app/docs or mail ops@genesy.app, e.g. now.</p>
     <span>gsk_0f6b</span><span>Read catalogue</span><span>Price 12.50</span><span>06:12</span><span>Open 449.40</span><span>v1.2.3</span>
     <input value="catalog:read"><code>stock.changed</code><pre>till_login</pre><kbd>pending_review</kbd>
     <span data-raw>lot_code_7</span><span hidden>store_only</span><span style="display:none">true</span>`
  )
  await expectNoRawValues(page)
})

for (const [name, html] of [
  ["scope key", "<td>catalog:read, till:login</td>"],
  ["event name", "<td>stock.changed, order.created</td>"],
  ["snake_case", "<span>pending_review</span>"],
  ["UUID", "<td>3f2b8c1e-9a4d-4e2b-8c1f-0a9b8c7d6e5f</td>"],
  ["boolean", "<td>false</td>"],
  ["ISO date", "<td>2026-11-04</td>"],
  ["ISO timestamp", "<td>2026-11-04T18:12:49Z</td>"],
  ["US date", "<td>11/4/2026, 6:12:49 PM</td>"],
  ["i18n key", "<span>invoices.statuses.pending</span>"],
  ["camelCase i18n key", "<button>orders.makeInvoice</button>"],
] as const)
  test(`raw values: catches a ${name}`, async ({ page }) => {
    await page.setContent(`<p>Fine text</p>${html}`)
    await expect(expectNoRawValues(page)).rejects.toThrow(/raw values/)
  })

test("raw values: an allowlist and a scope let legit codes through", async ({ page }) => {
  await page.setContent(`<main><span>AFM_LOT_7</span> <span>lot_7a</span></main><nav>till_login</nav>`)
  await expect(expectNoRawValues(page)).rejects.toThrow(/lot_7a/)
  await expectNoRawValues(page, { allow: [/lot_\w+/, "AFM_LOT_7"], within: "main" })
})
