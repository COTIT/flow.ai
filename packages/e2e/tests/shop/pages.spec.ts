import {
  expect,
  expectNoHorizontalScroll,
  expectNoRawValues,
  expectTapTargets,
  test,
} from "../../src/test"

const EMAIL = "support@africanfarmersmarketonline.com"

// AFM's content pages: each opens on its own h1, reads as words (no codes), fits a phone.
const PAGES = [
  { path: "/about", h1: /Fair, traceable pathways/, title: "About us" },
  { path: "/stories", h1: /The people behind every jar/, title: "Stories" },
  {
    path: "/suppliers",
    h1: "Partner with African Farmers Market",
    title: "Become a supplier",
  },
  { path: "/faq", h1: "Questions and answers", title: "Questions and answers" },
  { path: "/contact", h1: "Talk to us", title: "Contact" },
]

for (const { path, h1, title } of PAGES)
  test(`${path} opens on its heading, in words, at any width`, async ({
    page,
    isMobile,
  }) => {
    await page.goto(path)
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(h1)
    await expect(page).toHaveTitle(`${title} · African Farmers Market`)
    await expectNoRawValues(page, { allow: [EMAIL] })
    await expectNoHorizontalScroll(page)
    if (isMobile) await expectTapTargets(page)
  })

test("the header marks the page you are on", async ({ page, isMobile }) => {
  test.skip(isMobile, "phones: the links sit in the menu (next test)")
  await page.goto("/stories")
  const nav = page.getByRole("navigation", { name: "Main" })
  await expect(nav.getByRole("link", { name: "Stories" })).toHaveAttribute(
    "aria-current",
    "page"
  )
  await nav.getByRole("link", { name: "FAQ" }).click()
  await expect(page).toHaveURL("/faq", { timeout: 15_000 })
  await expect(nav.getByRole("link", { name: "FAQ" })).toHaveAttribute(
    "aria-current",
    "page"
  )
  await expect(page.getByRole("button", { name: "Open the menu" })).toBeHidden()
})

test("on a phone the menu opens, follows a link and closes", async ({
  page,
  isMobile,
}) => {
  test.skip(!isMobile, "desktop: links sit in the header")
  await page.goto("/contact")
  await expect(page.getByRole("navigation", { name: "Main" })).toBeHidden()
  await page.getByRole("button", { name: "Open the menu" }).click()
  const menu = page.getByRole("dialog", { name: "Main" })
  await expect(menu.getByRole("link", { name: "Contact" })).toHaveAttribute(
    "aria-current",
    "page"
  )
  await menu.getByRole("link", { name: "About" }).click()
  await expect(page).toHaveURL("/about", { timeout: 15_000 })
  await expect(menu).toBeHidden()
})

test("the menu works from the keyboard and Escape closes it", async ({
  page,
  isMobile,
}) => {
  test.skip(!isMobile, "the menu button shows on phones only")
  await page.goto("/faq")
  await page.getByRole("button", { name: "Open the menu" }).focus()
  await page.keyboard.press("Enter")
  const menu = page.getByRole("dialog", { name: "Main" })
  await expect(menu).toBeVisible()
  await page.keyboard.press("Escape")
  await expect(menu).toBeHidden()
})

test("FAQ answers open from the keyboard", async ({ page }) => {
  await page.goto("/faq")
  const answer = page.getByText(/within 14 days/)
  await expect(answer).toBeHidden()
  await page.getByText("Can I return an item?").focus()
  await page.keyboard.press("Enter")
  await expect(answer).toBeVisible()
})

test("forms that need a backend are honest email links, with a subject", async ({
  page,
}) => {
  await page.goto("/suppliers")
  await expect(page.getByRole("link", { name: "Email us" })).toHaveAttribute(
    "href",
    `mailto:${EMAIL}?subject=Supplier%20application`
  )
  // Swahili copy is marked as Swahili for screen readers.
  await expect(
    page.getByRole("heading", { name: "Kwa Kiswahili" })
  ).toBeVisible()
  await expect(page.locator('section[lang="sw"]')).toHaveCount(1)
  await expect(page.locator("form")).toHaveCount(0)

  await page.goto("/contact")
  await expect(page.getByRole("link", { name: EMAIL })).toHaveAttribute(
    "href",
    `mailto:${EMAIL}?subject=Talk%20to%20us`
  )
  await expect(page.getByText("There is no contact form yet")).toBeVisible()

  const footer = page.getByRole("contentinfo")
  await expect(
    footer.getByRole("link", { name: "Join our list by email" })
  ).toHaveAttribute(
    "href",
    `mailto:${EMAIL}?subject=Please%20add%20me%20to%20your%20list`
  )
})

test("about shows the markets still to come, or says there are none", async ({
  page,
}) => {
  await page.goto("/about")
  const events = page.getByRole("region", { name: "Find us at markets" })
  // Dates are real: before Dec 21 2026 the list shows, after it the info notice.
  await expect(
    events
      .getByRole("listitem")
      .first()
      .or(events.getByRole("status").filter({ hasText: "No markets planned" }))
  ).toBeVisible()
  await expect(events.getByText(/\d{4}-\d{2}-\d{2}/)).toHaveCount(0)
})

test("content pages read in Dutch and Swahili", async ({
  page,
  context,
  baseURL,
}) => {
  for (const [locale, h1] of [
    ["nl", "Vragen en antwoorden"],
    ["sw", "Maswali na majibu"],
  ] as const) {
    await context.addCookies([
      { name: "NEXT_LOCALE", value: locale, url: baseURL! },
    ])
    await page.goto("/faq")
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(h1)
    await expectNoRawValues(page, { allow: [EMAIL] })
  }
})
