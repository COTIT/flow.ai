import {
  expect,
  expectNoLayoutShift,
  expectNoMotion,
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
