import { expect, test, designs } from "./storefront-api"
import { fakeAgent, startText, watch } from "./voice"
import { orbAt, phase, s, revealHeader } from "./shell"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

test("one conversation survives four navigations: one socket, one session, quiet scene context", async ({
  page,
  api,
}) => {
  designs(api)
  const sessions = watch(page, "session")
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  const scenes = () =>
    agent.sent
      .filter((e) => e.type === "contextual_update")
      .map((e) => String(e.text))
      .filter((text) => text.includes("flow_context"))
      .map((text) => JSON.parse(text).flow_context.active_scene)
  // The new conversation hears the scene it starts in.
  await expect.poll(scenes).toEqual(["home"])
  // F2: Your Flow also has "Storefront Studio"
  // The header hides again after each use: reveal it for every click.
  const storefront = {
    click: async () =>
      (await revealHeader(page))
        .getByRole("link", { name: "Storefront", exact: true })
        .click(),
  }

  // Each scene is told once it settles (quick hops inside the 300 ms debounce collapse).
  const heard: string[] = ["home"]
  const step = async (scene: string) => {
    heard.push(scene)
    await expect.poll(scenes).toEqual(heard)
  }
  await storefront.click() // 1
  await orbAt(page, "compact")
  await step("storefront")
  await page.getByRole("button", { name: s.backHome }).click() // 2
  await orbAt(page, "ambient")
  await step("home")
  await storefront.click() // 3
  await orbAt(page, "compact")
  await step("storefront")
  await page.getByRole("button", { name: s.orbOpen }).click() // 4
  await orbAt(page, "immersive")
  await step("home")

  await expect(phase(page)).toHaveText(new RegExp(s.phase.live))
  expect(agent.connections).toBe(1)
  expect(sessions.filter((r) => r.method === "POST")).toHaveLength(1)
  expect(sessions.filter((r) => r.method === "DELETE")).toHaveLength(0)

  // Scene changes reached the agent as quiet context only: never a spoken turn.
  expect(agent.sent.filter((e) => e.type === "user_message")).toHaveLength(0)
})
