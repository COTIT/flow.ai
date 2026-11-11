import type { CDPSession, Page } from "@playwright/test"

import { v } from "../voice"
import type { Persona } from "./harness"

/**
 * Persona 10, the mobile thumb user: one hand, touch only, edges and corners, swipes, long
 * presses, the on-screen keyboard, rotations. Touch goes through CDP so moves and holds are real
 * touch events (Playwright's touchscreen only taps).
 */

export const SIZES = {
  se: { width: 320, height: 568 },
  small: { width: 375, height: 667 },
  phone: { width: 375, height: 812 },
  landscape: { width: 812, height: 375 },
} as const

/** Never sign out by accident, in any language. */
export const LOGOUT = /^\s*(log out|uitloggen|toka)\s*$/i

export class Touch {
  private constructor(private readonly cdp: CDPSession) {}
  static async on(page: Page) {
    return new Touch(await page.context().newCDPSession(page))
  }
  private send(type: string, points: { x: number; y: number }[]) {
    return this.cdp.send("Input.dispatchTouchEvent", {
      type: type as "touchStart",
      touchPoints: points.map((p) => ({ ...p, radiusX: 12, radiusY: 12, force: 1 })),
    })
  }
  async tap(x: number, y: number) {
    await this.send("touchStart", [{ x, y }])
    await this.send("touchEnd", [])
  }
  async hold(x: number, y: number, ms = 700) {
    await this.send("touchStart", [{ x, y }])
    await new Promise((r) => setTimeout(r, ms))
    await this.send("touchEnd", [])
  }
  /** A finger drag in `steps` moves (a swipe when fast, a scroll or pull when slow). */
  async swipe(from: { x: number; y: number }, to: { x: number; y: number }, steps = 8, ms = 120) {
    await this.send("touchStart", [from])
    for (let i = 1; i <= steps; i++) {
      await this.send("touchMove", [
        { x: from.x + ((to.x - from.x) * i) / steps, y: from.y + ((to.y - from.y) * i) / steps },
      ])
      await new Promise((r) => setTimeout(r, ms / steps))
    }
    await this.send("touchEnd", [])
  }
}

/** What a finger would hit at (x, y), and whether a person would tap it (never out of Flow). */
export async function at(page: Page, x: number, y: number) {
  return page.evaluate(
    ([x, y, logout]) => {
      const el = document.elementFromPoint(x!, y!) as HTMLElement | null
      if (!el) return { ok: false, what: "nothing" }
      const c = el.closest<HTMLElement>("a,button,[role=button],input,select,label,textarea")
      if (c instanceof HTMLAnchorElement) {
        const out =
          c.target === "_blank" || new URL(c.href, location.href).origin !== location.origin
        if (out) return { ok: false, what: "outbound link" }
      }
      const name = (c?.getAttribute("aria-label") ?? c?.textContent ?? "").trim()
      if (new RegExp(logout as string, "i").test(name)) return { ok: false, what: "log out" }
      return { ok: true, what: `${(c ?? el).tagName.toLowerCase()} ${name.slice(0, 30)}`.trim() }
    },
    [x, y, LOGOUT.source] as const
  )
}

/** A point where a thumb rests: mostly the bottom third, often near an edge or corner. */
export function thumbPoint(persona: Persona, width: number, height: number) {
  const edge = persona.chance(0.4)
  const x = edge
    ? persona.pick([persona.int(1, 14), persona.int(width - 14, width - 2)])
    : persona.int(1, width - 2)
  const y = persona.chance(0.7)
    ? persona.int(Math.floor(height * 0.6), height - 2)
    : edge
      ? persona.pick([persona.int(1, 14), persona.int(1, height - 2)])
      : persona.int(1, height - 2)
  return { x, y }
}

/**
 * The on-screen keyboard: the visual viewport shrinks by `share` of the screen and says so, as
 * a phone browser does. `0` puts it away.
 */
export async function keyboard(page: Page, share: number) {
  await page.evaluate((share) => {
    const view = window.visualViewport
    if (!view) return
    if (share <= 0) delete (view as unknown as Record<string, unknown>).height
    else {
      const h = Math.round(window.innerHeight * (1 - share))
      Object.defineProperty(view, "height", { configurable: true, get: () => h })
    }
    view.dispatchEvent(new Event("resize"))
  }, share)
}

/** The message box and Send stay above the keyboard (inside the visual viewport). */
export async function boxAboveKeyboard(page: Page) {
  return page.evaluate(
    ([label, send]) => {
      const vh = window.visualViewport?.height ?? innerHeight
      const box = document.querySelector<HTMLElement>(`[aria-label="${label}"]`)
      const btn = document.querySelector<HTMLElement>(`button[aria-label="${send}"]`)
      const r = box?.getBoundingClientRect()
      const s = btn?.getBoundingClientRect()
      return {
        vh: Math.round(vh),
        box: r ? Math.round(r.bottom) : null,
        send: s ? Math.round(s.bottom) : null,
        top: r ? Math.round(r.top) : null,
      }
    },
    [v.message, v.send] as const
  )
}

/** The floating header and its handle (touch). */
export const header = (page: Page) => page.locator("[data-flow-header]")
export const headerShown = async (page: Page) =>
  (await header(page).getAttribute("data-shown").catch(() => null)) === "true"
export const headerHandle = (page: Page) => page.locator("[data-flow-handle]")

/** The phone's conversation sheet, found by its raise/lower handle. */
export const sheetHandle = (page: Page) =>
  page.getByRole("button", { name: /^(Raise|Lower) the conversation$/ })
export const sheet = (page: Page) =>
  page.locator("section").filter({ has: sheetHandle(page) })
