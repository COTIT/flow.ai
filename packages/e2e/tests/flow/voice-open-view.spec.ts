// P4: the assistant opens a Flow scene or view by name (`open_flow_view`): navigation only, never a
// write, never another origin. SIMULATED voice and API.
import { expect, test } from "./storefront-api"
import { dock } from "./shell"
import { fakeAgent, startText, type Agent } from "./voice"

test.beforeEach(({ api }) => void api)

/** The agent calls open_flow_view; resolves with the tool's answer. */
async function openView(agent: Agent, view: string) {
  const id = `open-${agent.sent.length}-${view}`
  agent.emit({
    type: "client_tool_call",
    client_tool_call: {
      tool_name: "open_flow_view",
      tool_call_id: id,
      parameters: { view },
    },
  })
  await expect
    .poll(() =>
      agent.sent.some(
        (e) => e.type === "client_tool_result" && e.tool_call_id === id
      )
    )
    .toBe(true)
  const result = agent.sent.find(
    (e) => e.type === "client_tool_result" && e.tool_call_id === id
  )!
  return JSON.parse(String(result.result)) as Record<string, unknown>
}

test("“show me low stock”: the assistant opens Stock Watch; the conversation stays reachable", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  expect(await openView(agent, "stock")).toEqual({
    status: "opened",
    view: "stock",
    title: "Stock Watch",
  })
  await expect(page).toHaveURL(/\/scenes\/stock$/)
  await expect(
    page.getByRole("heading", { level: 1, name: "Stock Watch" })
  ).toBeVisible()
  await expect(dock(page)).toBeVisible()
})

test("a scene asked for by its title opens; asked again, it is already open", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  expect(await openView(agent, "my Morning Pulse")).toMatchObject({
    status: "opened",
    view: "pulse",
  })
  await expect(page).toHaveURL(/\/scenes\/pulse$/)
  expect(await openView(agent, "pulse")).toMatchObject({
    status: "already_open",
    view: "pulse",
  })
})

test("“take me home” from a scene goes home, like Back to home", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  await openView(agent, "orders")
  await expect(page).toHaveURL(/\/scenes\/orders$/)
  expect(await openView(agent, "home")).toMatchObject({
    status: "opened",
    view: "home",
  })
  await expect(page).toHaveURL(/\/$/)
  await expect(dock(page)).toBeVisible()
})

test("an unknown name or an outside address opens nothing and lists what can be opened", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  for (const view of [
    "settings",
    "https://evil.test/scenes/stock",
    "../scenes/stock",
  ]) {
    const answer = await openView(agent, view)
    expect(answer.status, view).toBe("not_found")
    expect(answer.available, view).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ view: "stock", title: "Stock Watch" }),
      ])
    )
  }
  expect(new URL(page.url()).pathname).toBe("/")
})
