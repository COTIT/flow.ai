import type { Page } from "@playwright/test"

import {
  expect,
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectTapTargets,
} from "../../src/test"

/**
 * Nothing on the page transitions or animates (the till has no animation, ever): every element
 * and pseudo-element has a 0 s transition and no animation.
 */
export async function expectNoTransitions(page: Page) {
  const moving = await page.evaluate(() =>
    [...document.querySelectorAll("*")].flatMap((el) =>
      [null, "::before", "::after"].flatMap((pseudo) => {
        const s = getComputedStyle(el, pseudo)
        const moves =
          s.transitionDuration.split(",").some((d) => parseFloat(d) > 0) ||
          s.animationName.split(",").some((n) => n.trim() !== "none")
        return moves ? [`${el.tagName.toLowerCase()}${pseudo ?? ""}`] : []
      })
    )
  )
  expect(moving, "elements that transition or animate").toEqual([])
}

/**
 * The till's look rules on the current screen, once it has settled. `{ shift: false }` after
 * a server answer to a tap (a notice it adds may land later than the 500 ms the shift
 * metric forgives); check the screen before the tap instead.
 */
export async function expectTillLook(page: Page, { shift = true } = {}) {
  await expectNoHorizontalScroll(page)
  await expectTapTargets(page)
  if (shift) await expectNoLayoutShift(page)
  await expectNoTransitions(page)
}

/** Types digits on the on-screen keypad. */
export async function tapKeys(page: Page, digits: string) {
  for (const d of digits)
    await page.getByRole("button", { name: d, exact: true }).click()
}
