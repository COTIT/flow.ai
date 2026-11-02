import { expect, expectNoHorizontalScroll, test } from "../../src/test"

const TITLE = "Every jar has a story. Every farmer gets their share."

test("with motion, the hero gets its pollen and the feature strip loops", async ({
  page,
}) => {
  await page.goto("/")
  await expect(page.locator(".hero canvas")).toBeAttached()
  await expect(page.getByLabel("What Genesy does")).toBeVisible()
  await expect(page.getByRole("heading", { level: 1 })).toHaveAccessibleName(
    TITLE
  )
  await expect(page.getByText("Traced from the")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test.describe("with reduced motion", () => {
  test.use({ reducedMotion: "reduce" })

  test("no animated pieces load; the words stay as written", async ({
    page,
  }) => {
    await page.goto("/")
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(TITLE)
    await expect(page.getByText("Farm to kitchen, in the open")).toBeVisible()
    await expect(
      page.getByRole("list", { name: "What Genesy does" })
    ).toBeVisible()
    await expect(page.locator("canvas")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})

test.describe("when an animated piece fails to load", () => {
  // React reports the error its boundary caught; the page itself keeps working.
  test.use({
    allow: { consolePrefix: ["Error: Failed to load chunk", "ChunkLoadError"] },
  })

  test("its place keeps the plain words, nothing goes blank", async ({
    page,
  }) => {
    // Abort the Particles chunk only (aborted requests do not count as page failures).
    await page.route("**/_next/static/chunks/*.js", async (route) => {
      const response = await route.fetch()
      const body = await response.text()
      if (body.includes("particleSpread")) return route.abort("aborted")
      return route.fulfill({ response, body })
    })
    await page.goto("/")
    await expect(page.getByRole("heading", { level: 1 })).toHaveAccessibleName(
      TITLE
    )
    await expect(page.getByLabel("What Genesy does")).toBeVisible()
    await expect(page.locator(".hero canvas")).toHaveCount(0)
    await expect(page.locator(".hero .copy")).toBeVisible()
  })
})
