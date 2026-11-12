import type { Locator, Page } from "@playwright/test"

import shellEn from "../../../../apps/flow/messages/en/shell.json" with { type: "json" }
import { expect } from "../../src/test"

/** Flow shell texts (header phase, dock, orb). */
export const s = shellEn.shell

export const orb = (page: Page) => page.locator("[data-orb]")
export const dock = (page: Page) => page.getByRole("region", { name: s.dock })
export const phase = (page: Page) => page.locator("[data-phase]")

/** Waits until the orb is in `mode` and its move has finished. */
export async function orbAt(
  page: Page,
  mode: "ambient" | "immersive" | "compact"
) {
  await expect(orb(page)).toHaveAttribute("data-orb", mode)
  await expect
    .poll(() => orb(page).evaluate((el) => el.getAnimations().length))
    .toBe(0)
  await expect(orb(page)).toBeVisible()
}

type Box = { x: number; y: number; width: number; height: number }
const overlap = (a: Box, b: Box) =>
  a.x < b.x + b.width - 0.5 &&
  b.x < a.x + a.width - 0.5 &&
  a.y < b.y + b.height - 0.5 &&
  b.y < a.y + a.height - 0.5

/** The orb is drawn exactly on its slot and covers none of `others`. */
export async function expectOrbClear(page: Page, others: Locator[]) {
  const mode = await orb(page).getAttribute("data-orb")
  const ball = (await orb(page).boundingBox())!
  const slot = (await page.locator(`[data-orb-slot="${mode}"]`).boundingBox())!
  const side = Math.min(slot.width, slot.height)
  expect(Math.abs(ball.width - side), "orb fills its slot").toBeLessThan(1.5)
  expect(
    Math.abs(ball.x + ball.width / 2 - (slot.x + slot.width / 2)),
    "orb centred on its slot"
  ).toBeLessThan(1.5)
  expect(
    Math.abs(ball.y + ball.height / 2 - (slot.y + slot.height / 2))
  ).toBeLessThan(1.5)
  // What is drawn: the orb layer clips the orb (a scrolled conversation can carry it out of view).
  const layer = (await orb(page).locator("..").boundingBox())!
  const top = Math.max(ball.y, layer.y)
  const bottom = Math.min(ball.y + ball.height, layer.y + layer.height)
  const drawn = { ...ball, y: top, height: Math.max(0, bottom - top) }
  for (const other of others) {
    const box = await other.boundingBox()
    if (box && drawn.height)
      expect(overlap(drawn, box), `orb covers ${other}`).toBe(false)
  }
}

/**
 * The header floats hidden until asked for (top edge, focus, handle): reveal it the pointer way
 * (to the top edge) before using one of its controls.
 */
export async function revealHeader(page: Page) {
  const header = page.locator("[data-flow-header]")
  const width = page.viewportSize()?.width ?? 1280
  // From below, so a second call still crosses the top edge (a move to the same point fires nothing).
  await page.mouse.move(width / 3, 200)
  await page.mouse.move(width / 3, 2)
  await expect(header).toHaveAttribute("data-shown", "true")
  // Settled: it slides in, so wait until two reads of its place agree.
  let last = -1
  await expect
    .poll(async () => {
      const y = Math.round((await header.boundingBox())?.y ?? -1)
      const same = y === last && y >= 0
      last = y
      return same
    })
    .toBe(true)
  return header
}

/** Where the orb rests on home while a conversation is live: orb-first home has no sheet, so its big ambient place at every size. */
export const homeOrb = (page: Page) => (void page, "ambient" as const)
