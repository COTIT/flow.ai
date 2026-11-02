import { seedSession } from "../../src/session"
import { test } from "../../src/test"

// Self test of the shared guard's consolePrefix: an allowed start passes, anything else fails.
test.describe(() => {
  test.use({ allow: { consolePrefix: ["Expected crash: "] } })

  test("a console error that starts with an allowed prefix passes", async ({
    page,
  }) => {
    await seedSession(page)
    await page.goto("/login")
    await page.evaluate(() =>
      console.error("Expected crash: details that change per build")
    )
  })

  test("a console error with another start still fails the test", async ({
    page,
  }) => {
    // The guard throws when the test ends; test.fail() turns that into the expected outcome.
    test.fail()
    await page.goto("/login")
    await page.evaluate(() =>
      console.error("Unexpected crash: Expected crash: not at the start")
    )
  })
})
