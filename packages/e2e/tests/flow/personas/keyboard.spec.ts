// Persona 9, the keyboard and screen-reader user: no mouse. Tab, Shift+Tab, Enter, Space, Escape
// and arrows through every flow. Checks focus (visible, never lost, returns after dialogs),
// names, live regions, reduced motion, 200% zoom and forced colours.
// SIMULATED API and voice (see harness.ts). Only what Playwright offers: no axe.
import type { Locator, Page } from "@playwright/test"

import discoveryEn from "../../../../../apps/flow/messages/en/discovery.json" with { type: "json" }
import { menuItem, moreMenu, sc } from "../scenes-api"
import { s as shell } from "../shell"
import {
  capsule,
  k,
  proposeStock,
  readBack,
  receipt,
  scene,
  stockRoutes,
} from "../task"
import { designs, revisionButton } from "../storefront-api"
import { startText, v, typeInstead } from "../voice"
import { expect, test, type Persona, type Step } from "./harness"

const d = discoveryEn.discovery

type Focus = {
  body: boolean
  name: string
  path: string
  /** A focus indicator someone can see: an outline or a ring (box-shadow). */
  indicator: boolean
  /** On screen, not clipped away. */
  onScreen: boolean
}

/** Steps per walk: PERSONA_STEPS wins; CI runs a short walk (suite budget), local runs the full one. */
const STEPS = Number(process.env.PERSONA_STEPS) || (process.env.CI ? 12 : 40)

/** Where focus is and whether a sighted keyboard user can see it. */
async function focusOf(page: Page): Promise<Focus> {
  return page.evaluate(() => {
    let el = document.activeElement as HTMLElement | null
    while (el?.shadowRoot?.activeElement)
      el = el.shadowRoot.activeElement as HTMLElement
    const path = location.pathname + location.search
    if (!el || el === document.body || el === document.documentElement)
      return { body: true, name: "", path, indicator: false, onScreen: false }
    const style = getComputedStyle(el)
    const outline =
      style.outlineStyle !== "none" &&
      parseFloat(style.outlineWidth) > 0 &&
      !/rgba\(\d+, \d+, \d+, 0\)|transparent/.test(style.outlineColor)
    const ring =
      style.boxShadow !== "none" &&
      !/^(rgba\(0, 0, 0, 0\)[^,]*,?\s*)+$/.test(style.boxShadow)
    // A focused input shows its caret and border; a frame's own document shows its own focus.
    const field = el.matches(
      "input, textarea, select, iframe, [contenteditable]"
    )
    // The content landmark (skip link / route target, tabindex -1) is a place, not a control.
    const landmark = el.matches("main#flow-content[tabindex='-1']")
    const box = el.getBoundingClientRect()
    const onScreen =
      box.width > 0 &&
      box.height > 0 &&
      box.bottom > 0 &&
      box.right > 0 &&
      box.top < innerHeight &&
      box.left < innerWidth
    const name = (
      el.getAttribute("aria-label") ||
      el.innerText ||
      el.getAttribute("title") ||
      el.tagName.toLowerCase()
    )
      .trim()
      .replace(/\s+/g, " ")
      .slice(0, 50)
    return {
      body: false,
      name,
      path,
      indicator: outline || ring || field || landmark,
      onScreen,
    }
  })
}

/** Visible, usable controls without an accessible name. */
async function unnamed(page: Page) {
  return page.evaluate(() => {
    const controls = document.querySelectorAll<HTMLElement>(
      'a[href], button, input:not([type="hidden"]), select, textarea, [role="button"], [role="tab"], [role="link"], [role="combobox"], [role="radio"], [role="checkbox"], [role="switch"], [role="menuitem"], [role="option"]'
    )
    const text = (id: string) =>
      id
        .split(/\s+/)
        .map((i) => document.getElementById(i)?.textContent ?? "")
        .join(" ")
        .trim()
    return [...controls].flatMap((el) => {
      if (
        el.closest('[aria-hidden="true"], [inert], [hidden]') ||
        !el.checkVisibility()
      )
        return []
      const labelled = el.getAttribute("aria-labelledby")
      const label =
        el.getAttribute("aria-label")?.trim() ||
        (labelled ? text(labelled) : "") ||
        (el as HTMLInputElement).labels?.[0]?.textContent?.trim() ||
        el.innerText?.trim() ||
        el.getAttribute("title")?.trim() ||
        el.querySelector("img[alt]")?.getAttribute("alt")?.trim() ||
        (el.matches("input[type=submit], input[type=button]")
          ? (el as HTMLInputElement).value
          : "") ||
        el.getAttribute("placeholder")?.trim()
      return label ? [] : [el.outerHTML.slice(0, 160)]
    })
  })
}

/**
 * Keyboard invariants on top of the harness's: a named control everywhere; after a keyboard
 * step focus is visible, on screen and not on <body> (a step that changed the address may drop
 * it to the document start, which a reader expects).
 */
type Sanity = {
  /** Body is fine: the step left the page or wrapped past its last control. */
  bodyOk?: boolean
  /** Skip the indicator check (P9-2: forced colours draw no ring). */
  noIndicator?: boolean
  /** The step scrolled the page by keyboard (Home/End/PageUp/PageDown/arrows): the person moved
   *  the view away from a focused control on purpose, so it may be off screen. */
  scrolled?: boolean
}

async function expectKeyboardSane(
  page: Page,
  before: string,
  trail: string[],
  sanity: Sanity = {}
) {
  const names = await unnamed(page)
  expect(names, "controls without an accessible name").toEqual([])
  const focus = await focusOf(page)
  if (focus.body) {
    const moved = focus.path !== before
    expect(
      moved || !!sanity.bodyOk,
      `focus lost to <body> on ${focus.path} (no navigation)\n${trail.slice(-6).join("\n")}`
    ).toBe(true)
    return
  }
  if (!sanity.noIndicator)
    expect(
      focus.indicator,
      `focus invisible on "${focus.name}" at ${focus.path}`
    ).toBe(true)
  if (!sanity.scrolled)
    expect(
      focus.onScreen,
      `focused "${focus.name}" is off screen at ${focus.path}`
    ).toBe(true)
}

/** Tabs until `target` has focus; false if the keyboard never reaches it. */
async function tabTo(
  page: Page,
  target: Locator,
  { max = 80, back = false } = {}
) {
  // Not on this page (e.g. the composer on /login): tabbing for it only walks focus out of the page.
  if (
    !(await target
      .first()
      .waitFor({ state: "attached", timeout: 2_000 })
      .then(
        () => true,
        () => false
      ))
  )
    return false
  for (let i = 0; i < max; i++) {
    if (
      await target
        .evaluate(
          (el) =>
            el === document.activeElement ||
            el.contains(document.activeElement),
          undefined,
          { timeout: 1_000 }
        )
        .catch(() => false)
    )
      return true
    await page.keyboard.press(back ? "Shift+Tab" : "Tab")
  }
  return false
}

function keyboardSteps(
  persona: Persona,
  page: Page,
  { noIndicator = false } = {}
): Step[] {
  const key = (name: string, keys: () => string[], weight: number): Step => ({
    name,
    weight,
    run: async () => {
      const k = keys()
      persona.log(`   keys: ${k.join(" ")} on "${(await focusOf(page)).name}"`)
      for (const key of k) await page.keyboard.press(key)
    },
  })
  const steps: Step[] = [
    key("Tab forward", () => Array(persona.int(1, 8)).fill("Tab"), 8),
    key("Shift+Tab back", () => Array(persona.int(1, 4)).fill("Shift+Tab"), 3),
    key("Enter on what has focus", () => ["Enter"], 5),
    key("Space on what has focus", () => [" "], 2),
    key("Escape", () => ["Escape"], 2),
    key(
      "arrows",
      () =>
        Array(persona.int(1, 3)).fill(
          persona.pick(["ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp"])
        ),
      2
    ),
    key("Home / End", () => [persona.pick(["Home", "End"])], 1),
    {
      name: "type into the composer by keyboard",
      weight: 2,
      run: async () => {
        const box = page.getByLabel(v.message)
        if (!(await tabTo(page, box, { max: 60 })))
          return persona.log("   composer unreachable")
        await page.keyboard.type(
          persona.pick([
            "Sales today?",
            "Which orders wait?",
            "help",
            "Stock below minimum",
          ])
        )
        await page.keyboard.press("Enter")
      },
    },
    {
      name: "Genesy proposes a stock change",
      weight: 2,
      run: async () => {
        if (
          !(await page
            .getByRole("button", { name: v.addFiles })
            .isVisible()
            .catch(() => false))
        )
          return
        persona.agent.call("stock_set_levels", {
          path: { product: "p-500" },
          body: {
            warehouses: [
              {
                warehouse_id: "wh-main",
                stock_lvl: persona.pick([110, 120, 130]),
              },
            ],
          },
        })
      },
    },
    {
      name: "tab to a scene and open it",
      weight: 2,
      run: async () => {
        const links = page
          .getByRole("region", { name: sc.yourFlow })
          .getByRole("link")
        const n = await links.count()
        if (!n) return
        if (await tabTo(page, links.nth(persona.int(0, n - 1))))
          await page.keyboard.press("Enter")
      },
    },
    {
      name: "tab to Back to home",
      weight: 1,
      run: async () => {
        const back = page.getByRole("button", { name: shell.backHome })
        if (await back.isVisible().catch(() => false))
          if (await tabTo(page, back, { back: true }))
            await page.keyboard.press("Enter")
      },
    },
    {
      name: "tab to Approve and press it",
      weight: 1,
      run: async () => {
        const approve = scene(page).getByRole("button", { name: v.approve })
        if (await approve.isVisible().catch(() => false))
          if (await tabTo(page, approve)) await page.keyboard.press("Enter")
      },
    },
    {
      name: "browser Back (Alt+Left)",
      weight: 1,
      run: () => persona.history("back"),
    },
  ]
  // The person scrolled focus out of view on purpose (Home/End, arrows, Page keys) and has not
  // moved focus since: a step that leaves focus where it was (Escape, Space on a non-control)
  // cannot bring it back, and must not be blamed for it (seed 1373302409: End, then Escape).
  let away = false
  return steps.map((step) => ({
    ...step,
    run: async () => {
      await page
        .evaluate(
          () =>
            ((window as unknown as { focusBefore?: Element | null }).focusBefore =
              document.activeElement)
        )
        .catch(() => {})
      const before = await page
        .evaluate(() => location.pathname + location.search)
        .catch(() => "")
      // Already on body (after a known P9-1 drop): Enter/Space there cannot lose more.
      const wasBody = (await focusOf(page).catch(() => ({ body: false }))).body
      // What Enter/Space last pressed (recorded at keydown): if it unmounted, body is P9-1 (known).
      await page
        .evaluate(
          () => ((window as unknown as { p9?: Element | null }).p9 = null)
        )
        .catch(() => {})
      await step.run()
      await page.waitForTimeout(120) // let React settle focus moves
      const gone = await page
        .evaluate(() => {
          const el = (window as unknown as { p9?: Element | null }).p9
          return !!el && el !== document.body && !el.isConnected
        })
        .catch(() => false)
      const tabbing = /^(Tab|Shift\+Tab)/.test(step.name)
      if (gone && (await focusOf(page)).body)
        persona.log("   (P9-1: focused control unmounted, focus on body)")
      const stayed = await page
        .evaluate(
          () =>
            (window as unknown as { focusBefore?: Element | null }).focusBefore ===
            document.activeElement
        )
        .catch(() => false)
      const scrolled = /^(Home \/ End|arrows|Page ?(Up|Down))/.test(step.name)
      away = scrolled || (away && stayed)
      await expectKeyboardSane(page, before, persona.trail, {
        bodyOk: gone || tabbing || wasBody,
        noIndicator,
        scrolled: away,
      })
      // Past the last control focus leaves the page; the next Tab must come back into it.
      if (tabbing && (await focusOf(page)).body) {
        await page.keyboard.press("Tab")
        expect((await focusOf(page)).body, "Tab never re-enters the page").toBe(
          false
        )
      }
    },
  }))
}

async function setup(
  page: Page,
  api: { on: (m: string, p: string, h: () => object) => unknown }
) {
  const stock =
    readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on(
    "GET",
    "/workspace/:ws/products/:id/stock-with-bins",
    () => stock.body as object
  )
  await stockRoutes(page)
}

/** PERSONA_TRAIL=1 prints the walk. */
async function walk(
  persona: Persona,
  page: Page,
  options: { noIndicator?: boolean } = {}
) {
  await page.addInitScript(() =>
    addEventListener(
      "keydown",
      (e) => {
        if (e.key === "Enter" || e.key === " ")
          (window as unknown as { p9?: Element | null }).p9 =
            document.activeElement
      },
      true
    )
  )
  await page.reload()
  try {
    await persona.run(keyboardSteps(persona, page, options), { count: STEPS })
  } finally {
    if (process.env.PERSONA_VERBOSE) console.log(persona.trail.join("\n"))
  }
}

test("keyboard user: no mouse through home, scenes, conversation and reviews", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await page.goto("/")
  await walk(persona, page)
})

test.describe("forced colours", () => {
  test.use({ forcedColors: "active" })
  test("keyboard user in high contrast (indicator: see P9-2)", async ({
    persona,
    page,
    api,
  }) => {
    await setup(page, api)
    await page.goto("/")
    await walk(persona, page, { noIndicator: true })
  })

  // P9-2: the shared Button draws focus with a box-shadow ring and a border colour (outline-none);
  // forced colours drop both, so a focused button looks unfocused.
  test("P9-2: in forced colours a focused button shows a focus indicator", async ({
    persona,
    page,
    api,
  }) => {
    void persona
    await setup(page, api)
    await page.goto("/")
    const logout = page.getByRole("button", { name: /log out/i })
    expect(await tabTo(page, logout)).toBe(true)
    expect((await focusOf(page)).indicator, "focus invisible on Log out").toBe(
      true
    )
  })
})

test.describe("200% zoom", () => {
  // 200% zoom of a 1280 px window lays out like a 640 px one at twice the pixels.
  test.use({ viewport: { width: 640, height: 400 }, deviceScaleFactor: 2 })
  test("keyboard user at 200% zoom", async ({ persona, page, api }) => {
    await setup(page, api)
    await page.goto("/")
    await walk(persona, page)
  })
})

// ---- Targeted probes ------------------------------------------------------------------------

test("probe: the first Tab lands on a visible control, and the whole page is reachable", async ({
  persona,
  page,
  api,
}) => {
  void persona
  await setup(page, api)
  await page.goto("/")
  await expect(page.getByRole("region", { name: sc.yourFlow })).toBeVisible()
  await page.keyboard.press("Tab")
  await expectKeyboardSane(page, "", [])
  for (const target of [
    page.getByRole("link", { name: "Storefront", exact: true }),
    page.getByRole("button", { name: typeInstead, exact: true }),
    page.getByRole("link", { name: d.hint.explore }),
    page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first(),
    // Orb-first home's dock holds the suggestions (the message box opens with the conversation).
    page.getByRole("region", { name: shell.dock }).getByRole("list").getByRole("button").first(),
  ])
    expect(
      await tabTo(page, target, { max: 120 }),
      `unreachable: ${target}`
    ).toBe(true)
})

// P9-1: when the focused control unmounts, focus drops to <body> (a, b, c).
test("P9-1a: dismissing the tip by keyboard keeps focus on the page", async ({
  persona,
  page,
  api,
}) => {
  void persona
  await setup(page, api)
  await page.goto("/")
  const dismiss = page.getByRole("button", { name: d.hint.dismiss })
  expect(await tabTo(page, dismiss)).toBe(true)
  await page.keyboard.press("Enter")
  await expect(dismiss).toHaveCount(0)
  expect(
    (await focusOf(page)).body,
    "focus lost to <body> after dismissing the tip"
  ).toBe(false)
})

test("probe: the reset dialog traps focus, Escape closes it, focus returns to its trigger", async ({
  persona,
  page,
  api,
}) => {
  void persona
  await setup(page, api)
  // "Reset Your Flow" shows once something is personalised: remove a scene, by keyboard.
  await page.goto("/scenes/sales")
  // Remove lives in the scene's More menu: Tab to More, open it, arrow to Remove, Enter.
  expect(await tabTo(page, moreMenu(page))).toBe(true)
  await page.keyboard.press("Enter")
  const remove = menuItem(page, sc.remove)
  for (
    let i = 0;
    i < 10 &&
    !(await remove
      .evaluate((el) => el === document.activeElement)
      .catch(() => false));
    i++
  )
    await page.keyboard.press("ArrowDown")
  await expect(remove).toBeFocused()
  await page.keyboard.press("Enter")
  await expect
    .poll(() => api.callsTo("PUT", "/workspace/w1/flow/scenes/sales").length)
    .toBe(1)
  await page.goto("/")
  const trigger = page
    .getByRole("region", { name: sc.yourFlow })
    .getByRole("button", { name: sc.reset })
  await expect(trigger).toBeVisible()
  expect(await tabTo(page, trigger, { max: 120 })).toBe(true)
  await page.keyboard.press("Enter")
  const dialog = page.getByRole("alertdialog")
  await expect(dialog).toBeVisible()
  for (const press of [
    "Tab",
    "Tab",
    "Tab",
    "Shift+Tab",
    "Shift+Tab",
    "Shift+Tab",
    "Shift+Tab",
  ]) {
    await page.keyboard.press(press)
    // Radix's focus guards (spans at the page edges) hand focus back a moment later.
    await expect
      .poll(
        () => dialog.evaluate((el) => el.contains(document.activeElement)),
        {
          message: `focus left the open dialog after ${press}`,
          timeout: 1_000,
        }
      )
      .toBe(true)
  }
  await page.keyboard.press("Escape")
  await expect(dialog).toHaveCount(0)
  await expect(trigger).toBeFocused()
})

test("P9-1b: review → approve → receipt by keyboard, focus never lost", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await proposeStock(page, persona.agent, 120)
  const approve = scene(page).getByRole("button", { name: v.approve })
  expect(await tabTo(page, approve), "Approve unreachable by keyboard").toBe(
    true
  )
  await page.keyboard.press("Enter")
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
  await page.waitForTimeout(200)
  const focus = await focusOf(page)
  expect(
    focus.body,
    "focus lost to <body> once Approve became the receipt"
  ).toBe(false)
  await expectKeyboardSane(page, "", [])
})

test("P9-1c: cancel a review by keyboard, focus never lost", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await proposeStock(page, persona.agent, 120)
  const cancel = scene(page).getByRole("button", {
    name: v.cancel,
    exact: true,
  })
  expect(await tabTo(page, cancel)).toBe(true)
  await page.keyboard.press("Enter")
  await expect(
    scene(page).getByRole("button", { name: v.approve })
  ).toHaveCount(0)
  await page.waitForTimeout(200)
  expect((await focusOf(page)).body, "focus lost to <body> after Cancel").toBe(
    false
  )
})

test("probe: the task line's Review button is reachable from a scene by keyboard", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await proposeStock(page, persona.agent, 120)
  // Leave by keyboard (a reload would end the conversation): home, then Stock Watch.
  const flow = page.getByRole("region", { name: sc.yourFlow })
  for (let i = 0; i < 3 && !(await flow.isVisible()); i++) {
    expect(
      await tabTo(page, page.getByRole("button", { name: shell.backHome }), {
        back: true,
      })
    ).toBe(true)
    await page.keyboard.press("Enter")
  }
  expect(
    await tabTo(page, flow.getByRole("link", { name: /Stock Watch/ }))
  ).toBe(true)
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL(/\/scenes\/stock$/)
  // P5: the review shows in place in the conversation beside the scene; where the conversation is
  // folded away (a lowered sheet) the task line's Review is the way to it.
  const review = capsule(page).getByRole("button", { name: k.review })
  if (await review.isVisible()) {
    expect(await tabTo(page, review)).toBe(true)
    await page.keyboard.press("Enter")
  }
  // A route change: Next's announcer reads the page; the review must then be reachable.
  await expect(
    scene(page).getByRole("button", { name: v.approve })
  ).toBeVisible()
  expect(
    await tabTo(page, scene(page).getByRole("button", { name: v.approve }))
  ).toBe(true)
})

test("probe: choosing a product by keyboard (clarify)", async ({
  persona,
  page,
  api,
}) => {
  await setup(page, api)
  await page.goto("/")
  await startText(page)
  await page.keyboard.type("Update the honey stock.")
  await page.keyboard.press("Enter")
  persona.agent.call("products_list", { query: { q: "honey" } })
  await expect.poll(persona.agent.results).toBe(1)
  persona.agent.say("I found three honey products. Which one did you mean?")
  // P5: the choice shows in place, in the conversation.
  await expect(page.getByRole("form", { name: k.clarify.title })).toBeVisible()
  const radios = page
    .getByRole("form", { name: k.clarify.title })
    .getByRole("radio")
  await expect(radios).toHaveCount(3)
  await expect(radios.first()).toBeVisible()
  expect(await tabTo(page, radios.first())).toBe(true)
  await page.keyboard.press("ArrowDown")
  await expect(radios.nth(1)).toBeFocused()
  await page.keyboard.press("Space")
  await expect(radios.nth(1)).toBeChecked()
  await expectKeyboardSane(page, "", [])
})

test("probe: the explorer's tabs work with arrows and Try it keeps focus", async ({
  persona,
  page,
  api,
}) => {
  void persona
  await setup(page, api)
  await page.goto("/explore")
  const first = page.getByRole("tab").first()
  expect(await tabTo(page, first)).toBe(true)
  await page.keyboard.press("ArrowRight")
  await expect(page.getByRole("tab").nth(1)).toBeFocused()
  await page.keyboard.press("Enter")
  await expectKeyboardSane(page, "/explore", [])
})

test("probe: an idle scene does not chatter in live regions", async ({
  persona,
  page,
  api,
}) => {
  void persona
  await setup(page, api)
  await page.goto("/scenes/pulse")
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await page.waitForTimeout(1_500)
  const changes = await page.evaluate(
    () =>
      new Promise<string[]>((resolve) => {
        const seen: string[] = []
        const live = () =>
          document.querySelectorAll(
            '[aria-live]:not([aria-live="off"]), [role="status"], [role="alert"], [role="log"], [role="timer"]'
          )
        const observer = new MutationObserver((records) => {
          for (const r of records) {
            const region = (
              r.target instanceof Element ? r.target : r.target.parentElement
            )?.closest(
              '[aria-live]:not([aria-live="off"]), [role="status"], [role="alert"], [role="log"]'
            )
            if (region) seen.push(region.textContent?.trim().slice(0, 60) ?? "")
          }
        })
        for (const el of live())
          observer.observe(el, {
            subtree: true,
            childList: true,
            characterData: true,
          })
        setTimeout(() => {
          observer.disconnect()
          resolve(seen)
        }, 10_000)
      })
  )
  // Nothing happened on screen, so a reader should hear (almost) nothing.
  expect(changes, "live regions announcing on an idle scene").toHaveLength(0)
})

test.describe("reduced motion", () => {
  test.use({ reducedMotion: "reduce" })
  test("probe: nothing animates endlessly with reduced motion, home or talking", async ({
    persona,
    page,
    api,
  }) => {
    void persona
    await setup(page, api)
    await page.goto("/")
    const endless = () =>
      page.evaluate(() =>
        document
          .getAnimations()
          .filter(
            (a) =>
              a.playState === "running" &&
              a.effect?.getComputedTiming().endTime === Infinity
          )
          .map((a) => {
            const t = (a.effect as KeyframeEffect | null)
              ?.target as Element | null
            return `${(a as CSSAnimation).animationName ?? a.constructor.name} on ${t?.tagName.toLowerCase()}.${t?.className.toString().slice(0, 60)}`
          })
      )
    await page.waitForTimeout(500)
    expect(await endless(), "endless animations on home").toEqual([])
    await startText(page)
    await page.waitForTimeout(500)
    expect(await endless(), "endless animations while talking").toEqual([])
  })
})

// P9-3: choosing another workspace in the header menu by keyboard drops focus to <body>: open,
// arrows and Escape (focus back on the trigger) work, but the switch itself loses focus.
test("P9-3: the workspace menu by keyboard: open, move, Escape, choose; focus stays", async ({
  persona,
  page,
  api,
}) => {
  void persona
  await setup(page, api)
  await page.goto("/")
  const menu = page.getByRole("combobox", { name: "Workspace" })
  expect(await tabTo(page, menu)).toBe(true)
  await page.keyboard.press("Enter")
  await expect(page.getByRole("listbox")).toBeVisible()
  await expect(
    page.getByRole("option", { name: "Ilundo Honey Co." })
  ).toBeFocused() // opened: the arrow moves from the current one
  await page.keyboard.press("ArrowDown")
  await page.keyboard.press("Escape")
  await expect(page.getByRole("listbox")).toHaveCount(0)
  await expect(menu).toBeFocused()
  await page.keyboard.press("Enter")
  await expect(page.getByRole("listbox")).toBeVisible()
  await expect(
    page.getByRole("option", { name: "Ilundo Honey Co." })
  ).toBeFocused() // opened: the arrow moves from the current one
  await page.keyboard.press("ArrowDown")
  await expect(
    page.getByRole("option", { name: "Ruaha Growers" })
  ).toBeFocused()
  await page.keyboard.press("Enter")
  await expect(menu).toContainText("Ruaha")
  await page.waitForTimeout(300)
  const focus = await focusOf(page)
  expect(focus.body, "focus lost to <body> after switching workspace").toBe(
    false
  )
  await expectKeyboardSane(page, "", [])
})

test("probe: the storefront studio tabs through, past the preview frame, to the dock", async ({
  persona,
  page,
  api,
}) => {
  void persona
  await setup(page, api)
  designs(api)
  await page.goto("/storefront")
  // Pick the draft revision by keyboard; its preview frame opens.
  const draftRow = page.getByRole("button", { name: revisionButton(2) })
  expect(
    await tabTo(page, draftRow, { max: 120 }),
    "revision unreachable by keyboard"
  ).toBe(true)
  await page.keyboard.press("Enter")
  await expect(page.locator('iframe[title="Shop preview"]')).toBeVisible()
  const seen: string[] = []
  const composer = page.getByLabel(v.message)
  let reached = false
  for (let i = 0; i < 150 && !reached; i++) {
    await page.keyboard.press("Tab")
    const f = await focusOf(page)
    seen.push(f.name)
    if (!f.body && !f.name.startsWith("iframe"))
      await expectKeyboardSane(page, "/storefront", seen)
    reached = await composer
      .evaluate((el) => el === document.activeElement)
      .catch(() => false)
  }
  expect(
    reached,
    `the dock is never reached by Tab; last stops: ${seen.slice(-8).join(" | ")}`
  ).toBe(true)
})
