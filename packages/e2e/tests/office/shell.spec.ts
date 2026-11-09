import {
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectTapTargets,
} from "../../src/test"
import { choose, expect, openMenu, paged, test, WS, WS2 } from "./fake-api"

const products = Array.from({ length: 120 }, (_, i) => ({
  id: `p-${i + 1}`,
  name: `Honey jar ${i + 1}`,
  product_nr: `${1000 + i}`,
  reference: `AFM-${i + 1}`,
  price_incl: "12.50",
  status: "active",
}))

const isPhone = (width = 0) => width < 768

test.beforeEach(({ api }) => {
  api.on("GET", "/workspace/:ws/products", ({ params, query }) => {
    const q = query.get("q") ?? ""
    const rows =
      params[0] === WS ? products.filter((p) => p.name.includes(q)) : []
    return paged(rows, Number(query.get("page") ?? 1))
  })
})

test("the menu marks the current page; a list pages through meta", async ({
  page,
}) => {
  await page.goto("/products")
  await openMenu(page)
  await expect(
    page
      .getByRole("navigation", { name: "Main menu" })
      .getByRole("link", { name: "Products" })
  ).toHaveAttribute("aria-current", "page")
  await page.keyboard.press("Escape")
  await expect(
    page.getByRole("cell", { name: "Honey jar 1", exact: true })
  ).toBeVisible()
  await expect(page.getByText("1–50 of 120")).toBeVisible()

  await page.getByRole("button", { name: "Next" }).click()
  await expect(page).toHaveURL(/page=2/)
  await expect(page.getByText("51–100 of 120")).toBeVisible()
  await page.getByRole("button", { name: "Next" }).click()
  await expect(page.getByText("101–120 of 120")).toBeVisible()
  await expect(page.getByRole("button", { name: "Next" })).toBeDisabled()
})

test("search sends q and resets to page 1", async ({ page, api }) => {
  await page.goto("/products?page=2")
  await page.getByRole("searchbox", { name: "Search products" }).fill("jar 7")
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL(/\?q=jar\+7$/)
  await expect(page.getByText("1–11 of 11")).toBeVisible()
  expect(api.calls.at(-1)?.path).toBe(`/workspace/${WS}/products`)
})

test("the top bar search opens the search page with the query", async ({
  page,
  api,
}) => {
  test.skip(
    isPhone(page.viewportSize()?.width),
    "the phone bar links to the search page instead"
  )
  api.on("GET", "/workspace/:ws/quick-search", () => ({
    data: { products: [], orders: [], invoices: [], lots: [], relations: [] },
  }))
  await page.goto("/orders")
  await page.getByRole("searchbox", { name: "Quick search" }).fill("jar 9")
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL(/\/search\?q=jar\+9$/)
  await expect(
    page.getByRole("heading", { level: 1, name: "Search" })
  ).toBeVisible()
})

test("the phone bar's search button goes to the search page", async ({
  page,
}) => {
  test.skip(
    !isPhone(page.viewportSize()?.width),
    "a desktop has the search field in the top bar"
  )
  await page.goto("/orders")
  await page.getByRole("link", { name: "Quick search" }).click()
  await expect(page).toHaveURL(/\/search$/)
  await expect(
    page.getByRole("searchbox", { name: "Quick search" })
  ).toBeVisible()
})

test("switching workspace refetches for that workspace only", async ({
  page,
  api,
}) => {
  await page.goto("/products")
  await expect(page.getByText("1–50 of 120")).toBeVisible()
  await openMenu(page)
  await choose(page, "Workspace", "Other Co")
  await expect(page.getByText("No products yet.")).toBeVisible()
  expect(api.callsTo("GET", `/workspace/${WS2}/products`)).toHaveLength(1)
})

test("the menu shows who is signed in, the open orders and the links to accept", async ({
  page,
  api,
}) => {
  const total = (n: number) => ({
    ...paged([{ id: "o" }]),
    meta: { ...paged([]).meta, total: n },
  })
  api.on("GET", "/workspace/:ws/orders", ({ query }) =>
    query.get("status") === "open" ? total(12) : paged([])
  )
  api.on("GET", "/workspace/:ws/supply-links", ({ query }) =>
    query.get("status") === "pending" && query.get("direction") === "incoming"
      ? total(2)
      : paged([])
  )
  await page.goto("/products")
  await openMenu(page)
  const nav = page.getByRole("navigation", { name: "Main menu" })
  await expect(nav.getByRole("link", { name: "Orders 12" })).toBeVisible()
  await expect(nav.getByRole("link", { name: "Supply 2" })).toBeVisible()
  const links = api.callsTo("GET", `/workspace/${WS}/supply-links`)
  expect(links.length).toBeGreaterThan(0)
  await expect(
    page.getByText("Romeo Tester").locator("visible=true")
  ).toBeVisible()
  const count = api.callsTo("GET", `/workspace/${WS}/orders`)[0]
  expect(count).toBeDefined()
})

test("logout returns to login", async ({ page }) => {
  await page.goto("/products")
  await openMenu(page)
  await page.getByRole("button", { name: "Log out" }).click()
  await expect(page).toHaveURL(/\/login$/)
})

test("signed-out visitor is sent to login", async ({ page }) => {
  await page.addInitScript(() => localStorage.removeItem("genesy.session"))
  await page.goto("/products")
  await expect(page).toHaveURL(/\/login$/)
})

test("phone: the menu slides in, goes to a page and closes", async ({
  page,
  api,
}) => {
  test.skip(
    !isPhone(page.viewportSize()?.width),
    "the sidebar is always shown on a desktop"
  )
  api.on("GET", "/workspace/:ws/products/stocks", () => paged([]))
  await page.goto("/products")
  await expect(page.getByRole("navigation", { name: "Main menu" })).toBeHidden()
  await page.getByRole("button", { name: "Open menu" }).click()
  const nav = page.getByRole("navigation", { name: "Main menu" })
  await expect(nav).toBeVisible()
  await expectTapTargets(page, { within: "[data-slot=sheet-content]" })
  await nav.getByRole("link", { name: "Stock" }).click()
  await expect(page).toHaveURL(/\/stock$/)
  await expect(nav).toBeHidden()
})

test("desktop: the sidebar and top bar are always there", async ({ page }) => {
  test.skip(isPhone(page.viewportSize()?.width), "phones use the menu sheet")
  await page.goto("/products")
  await expect(
    page.getByRole("navigation", { name: "Main menu" })
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Open menu" })).toBeHidden()
  await expect(page.getByRole("combobox", { name: "Workspace" })).toHaveText(
    /Ilundo Honey Co\./
  )
})

test("a list shows skeleton rows while it loads, never a loading line", async ({
  page,
  api,
}) => {
  let release!: () => void
  const held = new Promise<void>((r) => (release = r))
  await page.route("**/v1/workspace/*/products*", async (route) => {
    await held
    await api.handle(route)
  })
  await page.goto("/products")
  await expect(page.locator("[data-slot=skeleton]").first()).toBeVisible()
  await expect(page.getByText("Loading…")).toHaveCount(0)
  release()
  await expect(page.getByText("1–50 of 120")).toBeVisible()
})

test("no sideways scroll, no layout shift, big enough targets", async ({
  page,
}) => {
  await page.goto("/products")
  await expect(page.getByText("1–50 of 120")).toBeVisible()
  // The user's tile (desktop) or the phone bar's avatar has drawn.
  await expect(
    page.getByText("RT", { exact: true }).locator("visible=true")
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
  await expectNoLayoutShift(page)
  if (isPhone(page.viewportSize()?.width)) await expectTapTargets(page)
})

// Tablet and small-laptop widths sit between the 375 and 1280 projects: the top bar must fit
// beside the sidebar with nothing overlapping (the breadcrumb used to spill over the search).
for (const width of [800, 1024]) {
  test(`top bar fits at ${width} px: no overlap, no sideways scroll`, async ({
    page,
  }, info) => {
    test.skip(
      isPhone(info.project.use.viewport?.width),
      "tablet widths run on the desktop project"
    )
    await page.setViewportSize({ width, height: 800 })
    await page.goto("/products")
    const search = page.locator("[data-topbar]").getByRole("search")
    await expect(search).toBeVisible()
    const bar = await page.locator("[data-topbar]").boundingBox()
    const boxes = await page.locator("[data-topbar] > *").evaluateAll((els) =>
      els
        .filter(
          (el) =>
            el.getAttribute("aria-hidden") !== "true" &&
            el.getBoundingClientRect().width > 0
        )
        .map((el) => {
          const r = el.getBoundingClientRect()
          return {
            left: r.left,
            right: r.right,
            scroll: el.scrollWidth - el.clientWidth,
          }
        })
    )
    boxes
      .slice(1)
      .forEach((box, i) =>
        expect(
          box.left,
          `item ${i + 1} starts after item ${i} ends`
        ).toBeGreaterThanOrEqual(boxes[i]!.right)
      )
    for (const b of boxes)
      expect(b.scroll, "no item overflows its own box").toBeLessThanOrEqual(1)
    expect(boxes.at(-1)!.right).toBeLessThanOrEqual(bar!.x + bar!.width + 1)
    await expectNoHorizontalScroll(page)
  })
}
