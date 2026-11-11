import { expect, test, designs } from "./storefront-api"
import { fakeAgent, startText, v } from "./voice"
import { dock, expectOrbClear, orb, orbAt, phase, s } from "./shell"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.
// Design tests 1, 2 (storefront as the scene), 6, 7 and 10 (every project width).

test("one orb moves home → conversation → scene → conversation → home, never over the frame", async ({
  page,
  api,
}) => {
  designs(api)
  const agent = await fakeAgent(page, (text, a) => a.say(`Answer: ${text}`))
  await page.goto("/")
  const header = page.locator("[data-flow-header]")
  await orbAt(page, "ambient")
  await expect(orb(page)).toHaveCount(1)
  // The Genesy orb, its state in words for assistive tech only (the caption shows it on screen).
  const genesy = orb(page).locator("[data-genesy-orb]")
  await expect(genesy).toHaveCount(1)
  await expect(genesy).toHaveAttribute("data-state", "idle")
  await expect(genesy.getByRole("status")).toHaveText(s.orb.idle)
  await expectOrbClear(page, [
    header,
    dock(page),
    page.getByRole("heading", { level: 1 }),
    page.getByRole("button", { name: v.textInstead }),
  ])

  // 1: home → voice: the orb grows into the conversation.
  await startText(page)
  await orbAt(page, "immersive")
  await expectOrbClear(page, [header, dock(page), page.getByRole("log")])
  await expect(phase(page)).toHaveText(new RegExp(s.phase.live))

  // 2: a scene opens: the orb shrinks to the upper right of the content, same connection.
  await page.getByRole("link", { name: "Storefront" }).click()
  await expect(page).toHaveURL(/\/storefront$/)
  await orbAt(page, "compact")
  const heading = page.getByRole("heading", { level: 1 })
  await expect(heading).toBeVisible()
  await expectOrbClear(page, [header, dock(page), heading])
  const ball = (await orb(page).boundingBox())!
  const width = page.viewportSize()!.width
  expect(ball.x + ball.width / 2).toBeGreaterThan(width / 2)

  // 6: the compact orb opens the same conversation again.
  await page.getByRole("button", { name: s.orbOpen }).click()
  await expect(page).toHaveURL(/\/$/)
  await orbAt(page, "immersive")
  await expect(page.getByRole("log")).toBeVisible()

  // 7: back home: ambient orb, still connected.
  await page.getByRole("button", { name: s.backHome }).click()
  await orbAt(page, "ambient")
  await expect(phase(page)).toHaveText(new RegExp(s.phase.live))
  expect(agent.connections).toBe(1)
})

test.describe("reduced motion", () => {
  test.use({ reducedMotion: "reduce" })

  test("the orb changes place instantly", async ({ page, api }) => {
    designs(api)
    await fakeAgent(page)
    await page.goto("/")
    await orbAt(page, "ambient")
    await startText(page)
    await expect(orb(page)).toHaveAttribute("data-orb", "immersive")
    expect(await orb(page).evaluate((el) => el.getAnimations().length)).toBe(0)
    expect(await orb(page).evaluate((el) => getComputedStyle(el).transitionDuration)).toBe("0s")
    // The still orb's words stay off screen: nothing spills over the caption below the slot.
    await expect(orb(page).locator("[data-genesy-orb]")).toHaveAttribute("data-still", "true")
    const words = (await orb(page).locator(".genesy-orb-label").boundingBox())!
    expect(words.width).toBeLessThanOrEqual(1)
  })
})
