import type { Page } from "@playwright/test"

import flowEn from "../../../../../apps/flow/messages/en.json" with { type: "json" }
import { expect } from "../../../src/test"
import { v } from "../voice"
import { labels, type Persona } from "./harness"

/**
 * Persona 4, the "dumb" breaker: garbage text, empty sends, bursts, random taps, nonsense order.
 * These are its own invariants on top of the harness ones (`persona.check()`).
 */

/** Set by any of the script-looking strings below if the app ever ran them as code. */
const PWN = "__p4"

/** Garbage a careless (or curious) user pastes. No word the raw-error check looks for. */
export const GARBAGE: readonly string[] = [
  // markup and script that must stay text
  `<img src=x onerror="window.${PWN}='img'">`,
  `<script>window.${PWN}='script'</script>`,
  `"><svg onload="window.${PWN}='svg'"><b id="p4b">bold?</b>`,
  `[open me](javascript:window.${PWN}='md')`,
  `javascript:window.${PWN}='href'`,
  "{{constructor.constructor('return 1')()}} ${7*7} #{7*7}",
  // message-format and printf placeholders
  "{name} {count, plural, one {# order} other {# orders}} %s %d %n %%",
  // SQL-ish
  "'; DROP TABLE orders; --",
  `1' OR '1'='1" OR ""="`,
  // object-key breakers
  "__proto__",
  "constructor",
  "toString hasOwnProperty valueOf",
  "null",
  // RTL, bidi overrides, mixed scripts
  "مرحبا بالعالم، كم مبيعات اليوم؟ שלום עולם",
  "abc‮desrever‬ def ⁦isolate⁩",
  // emoji, ZWJ families, flags, skin tones
  "🍯🐝👨‍👩‍👧‍👦🏳️‍🌈🇹🇿👍🏿".repeat(40),
  // zalgo
  "Z̴̡̛̼̝̿a̶̢̛͎̓l̷̰̈́g̸̱̈o̵̺͝ ".repeat(60),
  // control characters and odd spaces
  "a\u0000b\u0007c\bd\u001be f g h",
  // one very long word, a very long URL
  "Honigglasverkaufsstatistik".repeat(200),
  `https://shop.genesy.app/${"x".repeat(4000)}?q=${"y".repeat(500)}`,
  // numbers that are not numbers
  "-0 1e309 0x1F 9007199254740993 ٣٤٥ ½",
  // wake phrase alone and odd
  "hey genesy",
  "Hey Genesy,,,!!!",
  "HEY GENESY hey genesy hey genesy",
]

/** Only zero-width characters: invisible, and `trim()` keeps them. */
export const ZERO_WIDTH = "\u200b\u200c\u200d\u2060".repeat(30)

/** Messages that are empty to a human. */
export const EMPTYISH: readonly string[] = [
  "",
  " ",
  "     ",
  "\t\t",
  "​‌‍⁠".repeat(30), // zero-width only
  "　  ", // ideographic, no-break, em spaces
  "﻿",
]

const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64"
)
type File = { name: string; mimeType: string; buffer: Buffer }
/** Files nobody should pick: odd names, wrong types, empty, too big. */
export const BAD_FILES: readonly File[] = [
  { name: `<img src=x onerror="window.${PWN}='file'">.png`, mimeType: "image/png", buffer: PNG },
  { name: "invoice\u202Egnp.exe", mimeType: "image/png", buffer: PNG },
  { name: `${"Honigglasverkaufsstatistik".repeat(9)}.png`, mimeType: "image/png", buffer: PNG },
  { name: "مرحبا 🍯 שלום.png", mimeType: "image/png", buffer: PNG },
  { name: "../../etc/passwd.png", mimeType: "image/png", buffer: PNG },
  { name: ".png", mimeType: "image/png", buffer: PNG },
  { name: "empty.png", mimeType: "image/png", buffer: Buffer.alloc(0) },
  { name: "huge.png", mimeType: "image/png", buffer: Buffer.alloc(11 * 1024 * 1024, 1) },
  { name: "setup.exe", mimeType: "application/x-msdownload", buffer: Buffer.from("MZ") },
  { name: "notes.csv", mimeType: "text/csv", buffer: Buffer.from("a,b\n\u0000,'; DROP TABLE x;--\n") },
  { name: "shelf.png", mimeType: "image/png", buffer: PNG },
]

/** 100k characters: one word, or words. */
export const huge = (words: boolean) =>
  words ? "honey sales today please ".repeat(4_000) : "h".repeat(100_000)

/** Runs before any page script: catches code the app should never run, and long freezes. */
export async function arm(page: Page) {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>
    w.__p4Longest = 0
    try {
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries())
          w.__p4Longest = Math.max(Number(w.__p4Longest), entry.duration)
      }).observe({ type: "longtask", buffered: true })
    } catch {
      // no longtask support: the freeze check simply sees 0
    }
  })
  const dialogs: string[] = []
  page.on("dialog", (d) => {
    dialogs.push(`${d.type()}: ${d.message()}`)
    void d.dismiss().catch(() => {})
  })
  return dialogs
}

const CRASHED = flowEn.flow.crashed
/** Words of a raw error or a broken render, never meant for a person to read. */
const RAW =
  /\b(TypeError|ReferenceError|SyntaxError|RangeError|Unhandled Runtime Error)\b|\[object Object\]|\bundefined\b|\bNaN\b|Unexpected token|Internal Server Error|Application error|client-side exception|ECONNREFUSED|at [\w$.]+ \(/

/** P4's own invariants, after each step on every open page. */
export async function expectNotBroken(persona: Persona, dialogs: string[]) {
  expect(dialogs, "a dialog opened (script ran from typed text?)").toEqual([])
  for (const page of persona.pages) {
    if (page.isClosed()) continue
    const found = await page.evaluate(([pwn, tray]) => {
      const w = window as unknown as Record<string, unknown>
      const vw = window.innerWidth
      // Lines a person reads: bubbles, notes, the dock, a task card. None may stick out or be cut.
      const sel =
        '[data-role="user"],[data-role="agent"],[data-role="action"],[data-role="link"],[data-task-state],section[aria-label] p,[role="alert"],[role="status"],[aria-label="' + tray + '"] > *'
      const outside = Array.from(document.querySelectorAll<HTMLElement>(sel))
        .filter((el) => el.offsetParent !== null && !el.closest(".sr-only"))
        .filter((el) => {
          const r = el.getBoundingClientRect()
          return (
            r.width > 2 && r.height > 2 &&
            (r.right > vw + 1 ||
              r.left < -1 ||
              // a deliberate ellipsis (with the full name in a title) is not cut off
              (el.scrollWidth > el.clientWidth + 1 &&
                getComputedStyle(el).textOverflow !== "ellipsis"))
          )
        })
        .map((el) => {
          const r = el.getBoundingClientRect()
          return `${el.tagName.toLowerCase()}[${el.dataset.role ?? el.getAttribute("role") ?? ""}] left=${Math.round(r.left)} right=${Math.round(r.right)} vw=${vw} scrollW=${el.scrollWidth} clientW=${el.clientWidth}`
        })
      // Text outside what the person typed (their own bubble may hold anything).
      const clone = document.body.cloneNode(true) as HTMLElement
      clone
        .querySelectorAll('[data-role="user"],[data-role="agent"],script,style,input,textarea')
        .forEach((el) => el.remove())
      return {
        pwned: w[pwn!] ?? null,
        injected: document.querySelectorAll('img[src="x"], svg[onload], #p4b').length,
        outside,
        text: clone.innerText,
        longest: Number(w.__p4Longest ?? 0),
        path: location.pathname,
        shell: !!document.querySelector("[data-flow-scroll]"),
      }
    }, [PWN, v.attachments] as const)
    expect(found.pwned, "typed text ran as code").toBeNull()
    expect(found.injected, "typed markup became real elements").toBe(0)
    expect(found.outside, "text sticks out of the screen or is cut off").toEqual([])
    expect(found.text, "the screen crashed").not.toContain(CRASHED)
    expect(found.text.match(RAW)?.[0] ?? null, "raw error text on screen").toBeNull()
    expect(found.longest, "the page froze (one long task)").toBeLessThan(3_000)
    // A reload may be under way: the shell must come back, not stay away.
    if (found.path !== "/login" && !found.shell)
      await expect(page.locator("[data-flow-scroll]"), `no Flow shell on ${found.path}`).toHaveCount(1, { timeout: 10_000 })
  }
}

/** A tap where a finger lands: anywhere, unless it would leave Flow or sign out. */
export async function tapAnywhere(persona: Persona, page: Page) {
  const size = page.viewportSize()!
  const x = persona.int(1, size.width - 2)
  const y = persona.int(1, size.height - 2)
  const target = await page.evaluate(
    ([x, y, logout]) => {
      const el = document.elementFromPoint(x as number, y as number)
      const c = el?.closest<HTMLElement>("a,button,[role=button],input,label")
      if (!el) return { ok: false, what: "nothing" }
      if (c instanceof HTMLAnchorElement) {
        const out =
          c.target === "_blank" || new URL(c.href, location.href).origin !== location.origin
        if (out) return { ok: false, what: "outbound link" }
      }
      const name = c?.getAttribute("aria-label") ?? c?.textContent ?? ""
      if (new RegExp(logout as string).test(name.trim())) return { ok: false, what: "log out" }
      return {
        ok: true,
        what: `${(c ?? el).tagName.toLowerCase()} ${name.trim().slice(0, 30)}`.trim(),
      }
    },
    [x, y, labels.logout.source] as const
  )
  persona.log(`   tap ${x},${y} → ${target.what}`)
  if (!target.ok) return
  if (await page.evaluate(() => matchMedia("(pointer: coarse)").matches))
    await page.touchscreen.tap(x, y).catch(() => {})
  else await page.mouse.click(x, y).catch(() => {})
}
