import type { Page } from "@playwright/test"

import { expect, expectTapTargets, test } from "../../src/test"

// The glass sizes from docs/design/web-redesign (glass.css), measured in the real office CSS
// bundle. Shared parts are found by `data-slot` (buttons by `group/button`), so a probe with the
// same slot and classes as the component exercises exactly the shipped rule.

const isPhone = (page: Page) => (page.viewportSize()?.width ?? 0) < 768

async function glassLogin(page: Page) {
  await page.goto("/login")
  await page.evaluate(() => document.body.classList.add("theme-glass"))
  // Buttons have `transition-all`; measure once the switch has settled.
  await page.evaluate(() => Promise.all(document.getAnimations().map((a) => a.finished)))
}

/** Adds markup to the page and returns the computed style of `[data-probe]` inside it. */
async function probe(page: Page, html: string, props: string[]) {
  return page.evaluate(
    ({ html, props }) => {
      const host = document.createElement("div")
      host.innerHTML = html
      document.body.append(host)
      const el = host.querySelector<HTMLElement>("[data-probe]")!
      const style = getComputedStyle(el)
      const out: Record<string, string> = Object.fromEntries(
        props.map((p) => [p, style.getPropertyValue(p)])
      )
      const box = el.getBoundingClientRect()
      host.remove()
      return Object.assign(out, {
        width: box.width,
        height: box.height,
        top: box.top,
        bottom: box.bottom,
      }) as Record<string, string> & Record<"width" | "height" | "top" | "bottom", number>
    },
    { html, props }
  )
}

test("buttons and fields take the glass sizes", async ({ page }) => {
  await glassLogin(page)
  const submit = page.getByRole("button", { name: "Log in" })
  const email = page.getByLabel("Email")
  const submitBox = (await submit.boundingBox())!
  const emailBox = (await email.boundingBox())!

  expect(emailBox.height).toBe(44)
  expect(await email.evaluate((el) => getComputedStyle(el).borderRadius)).toBe("12px")
  expect(await submit.evaluate((el) => getComputedStyle(el).borderRadius)).toBe("12px")
  // A phone gets 44 px controls (tap targets); the desktop keeps the drawn 40 px.
  expect(submitBox.height).toBe(isPhone(page) ? 44 : 40)

  const sm = await probe(
    page,
    `<button data-probe class="group/button h-7 px-2.5">Small</button>`,
    ["border-radius", "font-size"]
  )
  expect(sm.height).toBe(isPhone(page) ? 44 : 34)
  expect(sm["border-radius"]).toBe("10px")
  expect(sm["font-size"]).toBe("13px")

  const lg = await probe(
    page,
    `<button data-probe class="group/button h-9 px-2.5">Large</button>`,
    ["border-radius", "font-size"]
  )
  expect(lg.height).toBe(52)
  expect(lg["border-radius"]).toBe("14px")
  expect(lg["font-size"]).toBe("16px")

  const icon = await probe(
    page,
    `<button data-probe class="group/button size-8">x</button>`,
    []
  )
  expect([icon.width, icon.height]).toEqual(isPhone(page) ? [44, 44] : [40, 40])

  const select = await probe(
    page,
    `<button data-slot="select-trigger" data-size="default" data-probe class="data-[size=default]:h-8">All</button>`,
    ["border-radius"]
  )
  expect(select.height).toBe(44)
  expect(select["border-radius"]).toBe("12px")

  const area = await probe(page, `<textarea data-slot="textarea" data-probe></textarea>`, [
    "min-height",
    "border-radius",
  ])
  expect(area["min-height"]).toBe("96px")
  // A field padded for its icon keeps that padding.
  const search = await probe(
    page,
    `<input data-slot="input" data-probe class="h-10 pl-2.5">`,
    ["padding-left", "padding-right"]
  )
  // 10 px is its own pl-2.5; the default-field rule would have made it 14 px.
  expect(search["padding-left"]).toBe("10px")
  expect(search.height).toBe(isPhone(page) ? 44 : 40)
  expect(area["border-radius"]).toBe("12px")
})

test("badge tones, table cells and skeleton follow the mockup", async ({ page }) => {
  await glassLogin(page)
  const badge = await probe(
    page,
    `<span data-slot="badge" data-variant="teal" data-probe class="inline-flex h-5 items-center px-2">Released</span>`,
    ["border-radius", "font-size", "font-weight", "color", "background-color"]
  )
  expect(badge.height).toBe(24)
  expect(badge["font-size"]).toBe("12px")
  expect(badge["font-weight"]).toBe("600")
  expect(badge.color).toBe("rgb(1, 106, 115)")
  expect(badge["background-color"]).toBe("rgba(1, 144, 155, 0.13)")

  const tones = await page.evaluate(() =>
    ["gold", "red", "slate"].map((tone) => {
      const el = document.createElement("span")
      el.dataset.slot = "badge"
      el.dataset.variant = tone
      document.body.append(el)
      const color = getComputedStyle(el).color
      el.remove()
      return color
    })
  )
  expect(tones).toEqual(["rgb(122, 90, 30)", "rgb(180, 35, 24)", "rgb(58, 83, 120)"])

  const dot = await page.evaluate(() => {
    const el = document.createElement("span")
    el.dataset.slot = "badge"
    el.dataset.variant = "gold"
    document.body.append(el)
    const on = getComputedStyle(el, "::before").width
    el.dataset.dot = "false"
    const off = getComputedStyle(el, "::before").content
    el.remove()
    return { on, off }
  })
  expect(dot).toEqual({ on: "6px", off: "none" })

  const th = await probe(
    page,
    `<table><thead><tr><th data-slot="table-head" data-probe class="h-10 px-2">Order</th></tr></thead></table>`,
    ["font-size", "font-weight", "text-transform", "padding-left", "padding-top"]
  )
  expect(th).toMatchObject({
    "font-size": "11px",
    "font-weight": "700",
    "text-transform": "uppercase",
    "padding-left": "16px",
    "padding-top": "12px",
  })

  const td = await probe(
    page,
    `<table><tbody><tr><td data-slot="table-cell" data-probe class="p-2">#1042</td></tr></tbody></table>`,
    ["font-size", "padding-left", "padding-top"]
  )
  expect(td).toMatchObject({ "font-size": "14px", "padding-left": "16px", "padding-top": "14px" })

  const skel = await probe(
    page,
    `<div data-slot="skeleton" data-probe class="animate-pulse rounded-md bg-muted h-4"></div>`,
    ["background-image", "border-radius"]
  )
  expect(skel["background-image"]).toContain("linear-gradient")
  expect(skel["border-radius"]).toBe("8px")
})

test("a dialog is centred on a desktop and a bottom sheet on a phone", async ({ page }) => {
  await glassLogin(page)
  const dialog = await probe(
    page,
    `<div data-slot="dialog-content" data-probe class="fixed top-1/2 left-1/2 z-50 grid w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 gap-4 rounded-xl p-4 sm:max-w-sm"><p style="height:120px">Body</p></div>`,
    ["border-top-left-radius", "border-bottom-left-radius"]
  )
  const view = page.viewportSize()!
  if (isPhone(page)) {
    expect(dialog.width).toBe(view.width)
    expect(dialog.bottom).toBe(view.height)
    expect(dialog["border-top-left-radius"]).toBe("26px")
    expect(dialog["border-bottom-left-radius"]).toBe("0px")
  } else {
    expect(dialog.width).toBe(540)
    expect(Math.round(dialog.top + dialog.height / 2)).toBe(view.height / 2)
    expect(dialog["border-bottom-left-radius"]).toBe("26px")
  }
})

test("phone: fields, toggles, tabs, switches and checkboxes are 44 px targets; desktop keeps the drawn sizes", async ({ page }) => {
  await glassLogin(page)
  const phone = isPhone(page)
  const field = await probe(page, `<input data-slot="input" data-probe class="h-10 pl-2.5">`, [])
  expect(field.height).toBe(phone ? 44 : 40)
  const toggle = await probe(page, `<button data-probe class="group/toggle h-7 min-w-7 px-2.5">B</button>`, [])
  expect(toggle.height).toBe(phone ? 44 : 28)
  const tab = await probe(
    page,
    `<div data-slot="tabs-list" class="inline-flex h-8"><button data-slot="tabs-trigger" data-probe class="h-[calc(100%-1px)] px-1.5">Edit</button></div>`,
    []
  )
  expect(tab.height).toBe(phone ? 44 : 31)
  // The switch keeps its drawn track; only its invisible hit area grows.
  const hit = await page.evaluate(() => {
    const el = document.createElement("button")
    el.dataset.slot = "switch"
    el.className = "relative inline-flex after:absolute after:-inset-x-3 after:-inset-y-2"
    el.style.cssText = "width:32px;height:18.4px;padding:0"
    document.body.append(el)
    const box = el.getBoundingClientRect()
    const after = getComputedStyle(el, "::after")
    const out = { width: box.width, top: after.top, left: after.left }
    el.remove()
    return out
  })
  expect(hit.width).toBe(32)
  expect(hit.left).toBe("-12px")
  expect(parseFloat(hit.top)).toBeCloseTo(phone ? -12.8 : -8, 1)
})

test.describe("phone", () => {
  test("every control on the glass login form is at least 44 px", async ({ page }) => {
    test.skip(!isPhone(page), "tap targets are a phone check")
    await glassLogin(page)
    await expectTapTargets(page, { within: "form" })
  })
})
