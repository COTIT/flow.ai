import { expect, test, designs } from "./storefront-api"
import { fakeAgent, startText, watch } from "./voice"
import { orbAt, phase, s } from "./shell"

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
  const storefront = page.getByRole("link", { name: "Storefront" })

  await storefront.click() // 1
  await orbAt(page, "compact")
  await page.getByRole("button", { name: s.backHome }).click() // 2
  await orbAt(page, "ambient")
  await storefront.click() // 3
  await orbAt(page, "compact")
  await page.getByRole("button", { name: s.orbOpen }).click() // 4
  await orbAt(page, "immersive")

  await expect(phase(page)).toHaveText(new RegExp(s.phase.live))
  expect(agent.connections).toBe(1)
  expect(sessions.filter((r) => r.method === "POST")).toHaveLength(1)
  expect(sessions.filter((r) => r.method === "DELETE")).toHaveLength(0)

  // Scene changes reach the agent as quiet context only: ids, no spoken turn.
  await expect
    .poll(scenes)
    .toEqual(["home", "storefront", "home", "storefront", "home"])
  expect(agent.sent.filter((e) => e.type === "user_message")).toHaveLength(0)
})
