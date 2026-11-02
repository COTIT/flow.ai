import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test("the page tells who it is for, shows the passport and the promise", async ({
  page,
}) => {
  await page.goto("/")
  for (const name of [
    "Built for the people along the route.",
    "Scan the jar. Read its whole story.",
    "Plain rules the system keeps for you.",
    "Grow, pack or sell something worth tracing? Let’s talk.",
  ])
    await expect(page.getByRole("heading", { level: 2, name })).toBeVisible()
  for (const name of [
    "Farmer programmes & cooperatives",
    "Brands & shops",
    "Buyers",
  ])
    await expect(page.getByRole("heading", { level: 3, name })).toBeVisible()
  // The passport card is made-up example data, and says so.
  await expect(page.getByText("Example", { exact: true })).toBeVisible()
  // Every "Talk to us" button paints its label (a same-named Tailwind utility once made it transparent).
  for (const link of await page.getByRole("link", { name: "Talk to us" }).all())
    if (await link.isVisible())
      expect(
        await link.evaluate((a) => getComputedStyle(a).webkitTextFillColor)
      ).not.toBe("rgba(0, 0, 0, 0)")
  await expectNoHorizontalScroll(page)
})

test("the one call to action opens an email", async ({ page }) => {
  await page.goto("/")
  await expect(page.getByRole("link", { name: "Email us" })).toHaveAttribute(
    "href",
    "mailto:hello@genesy.app?subject=Genesy%20for%20my%20organisation"
  )
  await expect(
    page.getByRole("link", { name: "hello@genesy.app" })
  ).toBeVisible()
})

test("the footer leads to the shop and the sign-in pages", async ({ page }) => {
  await page.goto("/")
  const footer = page.getByRole("contentinfo")
  for (const [name, href] of [
    ["Buy honey", "https://shop.genesy.app"],
    ["Office", "https://office.genesy.app"],
    ["Field app", "https://field.genesy.app"],
    ["Till", "https://till.genesy.app"],
  ] as const)
    await expect(
      footer.getByRole("link", { name, exact: true })
    ).toHaveAttribute("href", href)
})

test("light and dark: the switch flips the theme", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" })
  await page.goto("/")
  const html = page.locator("html")
  await expect(html).not.toHaveClass(/\bdark\b/)
  await page
    .getByRole("button", { name: "Switch light or dark" })
    .first()
    .click()
  await expect(html).toHaveClass(/\bdark\b/)
  await expectNoHorizontalScroll(page)
})

for (const [path, lang, title] of [
  ["/nl", "nl", "Elke pot heeft een verhaal. Elke boer krijgt zijn deel."],
  ["/sw", "sw", "Kila chupa ina hadithi. Kila mkulima anapata sehemu yake."],
] as const)
  test(`${path} speaks ${lang}`, async ({ page }) => {
    await page.goto(path)
    await expect(page.locator("html")).toHaveAttribute("lang", lang)
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(title)
    await expectNoHorizontalScroll(page)
  })

test("language links move between /, /nl and /sw", async ({ page }) => {
  await page.goto("/")
  await page.getByRole("link", { name: "Nederlands" }).first().click()
  await expect(page).toHaveURL(/\/nl$/)
  await page.getByRole("link", { name: "English" }).first().click()
  await expect(page.locator("html")).toHaveAttribute("lang", "en")
})

test.describe("an unknown language", () => {
  test.use({ allow: { responses: [{ url: "/xx", status: 404 }] } })
  test("is a 404", async ({ page }) => {
    const response = await page.goto("/xx")
    expect(response?.status()).toBe(404)
    await expect(
      page.getByRole("heading", { level: 1, name: "This page does not exist" })
    ).toBeVisible()
    await page.getByRole("link", { name: "Kiswahili" }).click()
    await expect(page.locator("html")).toHaveAttribute("lang", "sw")
  })
})
