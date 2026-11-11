import { expect, test, designs } from "./storefront-api"
import { fakeAgent, startText, v } from "./voice"
import { dock, expectOrbClear, orb, orbAt, phase, s, revealHeader } from "./shell"

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
  // The approved design orb at every size: five SVG strands in a glass body, never the WebGL sheets.
  await expect(genesy.locator("svg path")).toHaveCount(5)
  await expect(genesy.locator("canvas")).toHaveCount(0)
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
  await (await revealHeader(page)).getByRole("link", { name: "Storefront" }).click()
  await expect(page).toHaveURL(/\/storefront$/)
  await orbAt(page, "compact")
  const heading = page.getByRole("heading", { level: 1 })
  await expect(heading).toBeVisible()
  await expectOrbClear(page, [header, dock(page), heading])
  const ball = (await orb(page).boundingBox())!
  const { width, height } = page.viewportSize()!
  if (width < 640)
    // Phone: the conversation sheet holds it, in its peek at the bottom (PhoneSheet board).
    expect(ball.y).toBeGreaterThan(height / 2)
  else expect(ball.x + ball.width / 2).toBeGreaterThan(width / 2)

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

test("opening the conversation from the orb moves focus into it, never onto a hidden element", async ({
  page,
  api,
}) => {
  designs(api)
  await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  await (await revealHeader(page)).getByRole("link", { name: "Storefront" }).click()
  await orbAt(page, "compact")
  await page.getByRole("button", { name: s.orbOpen }).click()
  await orbAt(page, "immersive")
  const focus = () =>
    page.evaluate(() => {
      const el = document.activeElement
      return {
        hidden: !!el?.closest('[aria-hidden="true"]'),
        body: el === document.body,
        text: el?.textContent ?? "",
      }
    })
  await expect.poll(focus).toEqual({ hidden: false, body: false, text: s.conversation })
})
