import type { Locator } from "@playwright/test"

/** Keyboard focus on `target` (Tab first, so focus counts as keyboard), and how it is drawn. */
export async function focusLook(target: Locator) {
  await target.page().keyboard.press("Tab")
  await target.focus()
  return target.evaluate((el) => {
    const s = getComputedStyle(el)
    return {
      visible: el.matches(":focus-visible"),
      outline: s.outlineStyle,
      width: parseFloat(s.outlineWidth),
      ring: s.boxShadow,
    }
  })
}
