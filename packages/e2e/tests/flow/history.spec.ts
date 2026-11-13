import type { Page } from "@playwright/test"

import { expect, test } from "./storefront-api"
import { fakeAgent, startText, send, v, watch, FLOW_SIGNED_URL } from "./voice"
import { expectTapTargets } from "../../src/test"
import { s } from "./shell"

test.beforeEach(({ api }) => void api)

test.use({
  permissions: ["microphone"],
  launchOptions: {
    args: [
      "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream",
    ],
  },
})

const first = "019b0000-0000-7000-8000-000000000001"
const second = "019b0000-0000-7000-8000-000000000002"
const summary = (id = first, state = "active") => ({
  id,
  title: id === first ? "Harvest planning" : "Last week's delivery",
  state,
  revision: 2,
  coverage: "partial",
  last_activity_at: "2026-11-13T08:00:00Z",
  expires_at: "2026-12-13T08:00:00Z",
})

async function historyApi(page: Page) {
  const commands: Record<string, unknown>[] = []
  let issued = 0
  await page.route("**/api/voice/session", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    if (
      route.request().method() !== "POST" ||
      body.command === "heartbeat" ||
      body.command === "dropped"
    )
      return route.fallback()
    await route.fulfill({
      json: {
        sessionId: "flow-session",
        conversationId: "flow-conversation",
        signedUrl: FLOW_SIGNED_URL,
        history: {
          conversationId:
            body.logicalConversationId ||
            (issued++ === 0 ? first : "019b0000-0000-7000-8000-000000000003"),
          sourceId: "source-1",
          userId: "u1",
          context: JSON.stringify({
            correctedQuantity: commands.some(
              (c) =>
                c.command === "events" &&
                JSON.stringify(c.events).includes("24 crates")
            )
              ? 24
              : 12,
          }),
          revision: 2,
          coverage: "partial",
        },
      },
    })
  })
  let deleted = false
  let ended = false
  await page.route("**/api/voice/history", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    commands.push(body)
    expect(route.request().headers().authorization).toBe("Bearer e2e-token")
    expect(body.workspaceId).toBe("w1")
    let response: unknown
    if (body.command === "list")
      response = {
        conversations: deleted
          ? []
          : [
              summary(first, ended ? "ended" : "active"),
              summary(second, "ended"),
            ],
      }
    else if (body.command === "get")
      response = {
        conversation: summary(
          String(body.logicalConversationId),
          ended
            ? "ended"
            : body.logicalConversationId === second
              ? "ended"
              : "active"
        ),
        sources: [],
      }
    else if (body.command === "messages")
      response = {
        messages: [
          {
            id: "m1",
            source_id: "source-1",
            key: "turn-1",
            ordinal: 1,
            role: "user",
            text: "Plan tomorrow's harvest",
            modality: "text",
            status: "complete",
            interrupted: false,
            provenance: "final",
          },
          {
            id: "m2",
            source_id: "source-1",
            key: "turn-2",
            ordinal: 2,
            role: "agent",
            text: "Use the corrected quantity: 24 crates.",
            modality: "voice",
            status: "complete",
            interrupted: true,
            provenance: "final",
          },
        ],
        revision: 2,
        has_more: false,
        through_sequence: 2,
        gaps: [],
      }
    else if (body.command === "events") response = { status: "saved" }
    else if (body.command === "delete") {
      deleted = true
      response = {}
    } else if (body.command === "end") {
      ended = true
      response = {}
    } else response = { conversation: summary() }
    await route.fulfill({ json: response })
  })
  return commands
}

async function openHistory(page: Page) {
  await expect(page).toHaveURL(/\/$/)
  await page.getByRole("button", { name: s.showHeader, exact: true }).click()
  await page
    .getByRole("link", { name: "Conversation history", exact: true })
    .click()
  await expect(
    page.getByRole("heading", { name: "Conversation history", exact: true })
  ).toBeVisible()
}

test("history browsing preserves live socket, draft and transcript; history never replays actions", async ({
  page,
}) => {
  const commands = await historyApi(page)
  const agent = await fakeAgent(page)
  const sessions = watch(page, "session")
  const tools = watch(page, "tools")
  const confirms = watch(page, "confirm")
  await page.goto("/")
  await startText(page)
  agent.say("Current live conversation")
  await page.getByLabel(v.message).fill("Unsent draft")
  agent.call("orders_create", { body: { reference: "Flow e2e order" } })
  await expect(
    page.getByText("Confirm action 135790", { exact: true }).first()
  ).toBeVisible()
  const beforeTools = tools.length
  await openHistory(page)
  await page.getByRole("button", { name: /Harvest planning/ }).click()
  await expect(
    page.getByText("Use the corrected quantity: 24 crates.")
  ).toBeVisible()
  await expect(
    page.getByText("Some conversation history may be missing.")
  ).toBeVisible()
  await expect(page.getByLabel(v.message)).toHaveValue("Unsent draft")
  expect(agent.connections).toBe(1)
  expect(sessions.filter((r) => r.method === "DELETE")).toHaveLength(0)
  expect(tools).toHaveLength(beforeTools)
  expect(confirms).toHaveLength(0)
  await expect(
    page.getByRole("button", { name: v.send, exact: true })
  ).toBeDisabled()
  expect(
    commands.every((c) =>
      ["list", "get", "messages", "events"].includes(String(c.command))
    )
  ).toBe(true)
  await expectTapTargets(page, { within: "[data-history]" })
})

test("search, ended read-only history and confirmed delete", async ({
  page,
}) => {
  const commands = await historyApi(page)
  await page.goto("/history")
  await expect(
    page.getByRole("heading", { name: "Conversation history", exact: true })
  ).toBeVisible()
  await page.getByLabel("Search conversations").fill("harvest")
  await page.getByRole("button", { name: "Search", exact: true }).click()
  await expect
    .poll(() => commands.some((c) => c.command === "list" && c.q === "harvest"))
    .toBe(true)
  await page.getByRole("button", { name: /Last week's delivery/ }).click()
  await expect(
    page.getByRole("button", { name: "Continue with text", exact: true })
  ).toHaveCount(0)
  await page.getByLabel("Search messages").fill("24 crates")
  await page.getByLabel("Search messages").press("Enter")
  await expect
    .poll(() =>
      commands.some(
        (c) =>
          c.command === "messages" && c.q === "24 crates" && c.revision === 2
      )
    )
    .toBe(true)
  await page
    .getByRole("button", { name: "Delete conversation", exact: true })
    .click()
  await expect(page.getByRole("alertdialog")).toBeVisible()
  expect(commands.some((c) => c.command === "delete")).toBe(false)
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Delete", exact: true })
    .click()
  await expect
    .poll(() => commands.some((c) => c.command === "delete"))
    .toBe(true)
})

test.describe("authenticated continuity bootstrap", () => {
  test("current correction reaches real SDK bootstrap across reload, text and voice; New gets a new id", async ({
    page,
  }) => {
    test.setTimeout(60_000)
    const commands = await historyApi(page)
    const sessions = watch(page, "session")
    const agent = await fakeAgent(page)
    await page.goto("/")
    await startText(page)
    expect(
      agent.sent.find((e) => e.type === "conversation_initiation_client_data")
        ?.dynamic_variables
    ).toMatchObject({
      flow_history_context: JSON.stringify({ correctedQuantity: 12 }),
    })
    await send(page, "Correction: 24 crates, not 12")
    await expect
      .poll(() =>
        commands.some(
          (c) =>
            c.command === "events" &&
            JSON.stringify(c.events).includes("24 crates")
        )
      )
      .toBe(true)
    await page.getByRole("button", { name: v.endCall, exact: true }).click()
    await expect(page.getByRole("button", { name: v.addFiles })).toHaveCount(0)
    expect(commands.some((c) => c.command === "end")).toBe(false)
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            JSON.parse(sessionStorage.getItem("flow.conversation") || "{}").id
        )
      )
      .toBe(first)
    await page.reload()
    await expect
      .poll(() => commands.filter((c) => c.command === "messages").length)
      .toBeGreaterThan(0)
    expect(agent.connections).toBe(1)
    await page.goto("/history")
    await page.getByRole("button", { name: /Harvest planning/ }).click()
    await page
      .getByRole("button", { name: "Continue with text", exact: true })
      .click()
    await expect.poll(() => agent.connections).toBe(2)
    const starts = () =>
      agent.sent.filter((e) => e.type === "conversation_initiation_client_data")
    expect(starts().at(-1)?.dynamic_variables).toMatchObject({
      flow_history_context: JSON.stringify({ correctedQuantity: 24 }),
    })
    const lastStart = () =>
      sessions
        .filter(
          (r) =>
            r.method === "POST" && !(r.body as { command?: string }).command
        )
        .at(-1)?.body
    expect(lastStart()).toMatchObject({
      logicalConversationId: first,
      history: true,
      textOnly: true,
    })
    await openHistory(page)
    await page.getByRole("button", { name: /Harvest planning/ }).click()
    await page
      .getByRole("button", { name: "Continue with voice", exact: true })
      .click()
    await expect.poll(() => agent.connections).toBe(3)
    expect(lastStart()).toMatchObject({
      logicalConversationId: first,
      textOnly: false,
    })
    expect(starts().at(-1)?.dynamic_variables).toMatchObject({
      flow_history_context: JSON.stringify({ correctedQuantity: 24 }),
    })
    await openHistory(page)
    await page
      .getByRole("button", { name: "New conversation", exact: true })
      .click()
    await expect.poll(() => agent.connections).toBe(4)
    expect(lastStart()).not.toHaveProperty("logicalConversationId")
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            JSON.parse(sessionStorage.getItem("flow.conversation") || "{}").id
        )
      )
      .toBe("019b0000-0000-7000-8000-000000000003")
    await openHistory(page)
    await page.getByRole("button", { name: /Harvest planning/ }).click()
    await page
      .locator("[data-history]")
      .getByRole("button", { name: "End conversation", exact: true })
      .click()
    await expect
      .poll(() =>
        commands.some(
          (c) => c.command === "end" && c.logicalConversationId === first
        )
      )
      .toBe(true)
    // Ending the browsed conversation leaves a different live conversation connected.
    expect(agent.connections).toBe(4)
    await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
    await expect(
      page.getByRole("button", { name: "Continue with text", exact: true })
    ).toHaveCount(0)
  })
})

test("loading, empty and message search stay within history", async ({
  page,
}) => {
  await historyApi(page)
  let release: () => void = () => {}
  const waiting = new Promise<void>((resolve) => {
    release = resolve
  })
  await page.route("**/api/voice/history", async (route) => {
    const body = route.request().postDataJSON()
    if (body.command !== "list") return route.fallback()
    await waiting
    await route.fulfill({ json: { conversations: [] } })
  })
  await page.goto("/history")
  await expect(
    page.getByText("Loading conversation history…", { exact: true })
  ).toBeVisible()
  release()
  await expect(
    page.getByText("No saved conversations yet.", { exact: true })
  ).toBeVisible()
})

test.describe("history request failures", () => {
  test.use({
    allow: {
      responses: [
        { url: "/api/voice/history", status: 503 },
        { url: "/api/voice/history", status: 401 },
      ],
    },
  })
  test("failed history retries without starting a call; expired auth returns to login", async ({
    page,
  }) => {
    await historyApi(page)
    const agent = await fakeAgent(page)
    let status = 503
    await page.route("**/api/voice/history", (route) =>
      route.fulfill({ status, json: { message: "Unavailable" } })
    )
    await page.goto("/history")
    await expect(
      page.locator("[data-history]").getByRole("alert")
    ).toBeVisible()
    expect(agent.connections).toBe(0)
    status = 401
    await page.getByRole("button", { name: "Try again", exact: true }).click()
    await expect(page).toHaveURL(/\/login\?next=%2Fhistory$/)
    expect(agent.connections).toBe(0)
  })
})

for (const [locale, title] of [
  ["nl", "Gespreksgeschiedenis"],
  ["sw", "Historia ya mazungumzo"],
] as const) {
  test(`${locale} history uses translated controls`, async ({
    page,
    context,
    baseURL,
  }) => {
    await context.addCookies([
      { name: "NEXT_LOCALE", value: locale, url: baseURL! },
    ])
    await historyApi(page)
    await page.goto("/history")
    await expect(
      page.getByRole("heading", { name: title, exact: true })
    ).toBeVisible()
    await expectTapTargets(page, { within: "[data-history]" })
  })
}

for (const change of ["workspace", "logout"] as const) {
  test(`${change} clears the scoped conversation pointer`, async ({ page }) => {
    await historyApi(page)
    await fakeAgent(page)
    await page.goto("/")
    await startText(page)
    await expect
      .poll(() =>
        page.evaluate(() => sessionStorage.getItem("flow.conversation"))
      )
      .not.toBeNull()
    await page.getByRole("button", { name: v.endCall, exact: true }).click()
    await page.getByRole("button", { name: s.showHeader, exact: true }).click()
    if (change === "workspace") {
      await page
        .getByRole("combobox", { name: "Workspace", exact: true })
        .click()
      await page
        .getByRole("option", { name: "Ruaha Growers", exact: true })
        .click()
    } else {
      await page.getByRole("button", { name: "Log out", exact: true }).click()
      await expect(page).toHaveURL(/\/login$/)
    }
    await expect
      .poll(() =>
        page.evaluate(() => sessionStorage.getItem("flow.conversation"))
      )
      .toBeNull()
  })
}

test("conversation and message pagination preserve the selected revision", async ({
  page,
}) => {
  const commands = await historyApi(page)
  await page.route("**/api/voice/history", async (route) => {
    const body = route.request().postDataJSON()
    if (body.command === "list")
      return route.fulfill({
        json: {
          conversations: [summary(body.cursor ? second : first)],
          has_more: !body.cursor,
          cursor: body.cursor ? null : "next-conversation",
        },
      })
    if (body.command === "messages")
      return route.fulfill({
        json: {
          messages: [
            {
              id: body.cursor ? "page-2" : "page-1",
              source_id: "source-1",
              key: body.cursor ? "page-2" : "page-1",
              ordinal: body.cursor ? 2 : 1,
              role: "agent",
              text: body.cursor ? "Second saved page" : "First saved page",
              modality: "text",
              status: "complete",
              provenance: "final",
            },
          ],
          revision: 2,
          has_more: !body.cursor,
          cursor: body.cursor ? null : "next-message",
          gaps: [],
          through_sequence: 2,
        },
      })
    return route.fallback()
  })
  await page.goto("/history")
  await page
    .getByRole("button", { name: "More conversations", exact: true })
    .click()
  await expect(
    page.getByRole("button", { name: /Last week's delivery/ })
  ).toBeVisible()
  await page.getByRole("button", { name: /Harvest planning/ }).click()
  await page.getByRole("button", { name: "More messages", exact: true }).click()
  await expect(
    page
      .locator("[data-history] [data-role=agent]")
      .filter({ hasText: "First saved page" })
  ).toBeVisible()
  await expect(
    page
      .locator("[data-history] [data-role=agent]")
      .filter({ hasText: "Second saved page" })
  ).toBeVisible()
  expect(
    commands
      .filter((c) => c.command === "get")
      .every((c) => c.logicalConversationId === first)
  ).toBe(true)
})

for (const target of ["active", "other"] as const) {
  test(`deleting ${target} history respects the live voice call and ignores stale transport callbacks`, async ({
    page,
  }) => {
    const resources = () =>
      page.evaluate(
        () =>
          Reflect.get(window, "historyResourceState")() as {
            sockets: number
            openSockets: number
            closedSockets: number
            liveMicrophones: number
          }
      )
    const commands = await historyApi(page)
    const agent = await fakeAgent(page)
    await page.addInitScript(() => {
      const sockets: WebSocket[] = []
      const tracks: MediaStreamTrack[] = []
      const NativeWebSocket = window.WebSocket
      window.WebSocket = class extends NativeWebSocket {
        constructor(url: string | URL, protocols?: string | string[]) {
          super(url, protocols)
          if (String(url).startsWith("wss://api.elevenlabs.io/"))
            sockets.push(this)
        }
      }
      const capture = navigator.mediaDevices.getUserMedia.bind(
        navigator.mediaDevices
      )
      navigator.mediaDevices.getUserMedia = async (constraints) => {
        const stream = await capture(constraints)
        tracks.push(...stream.getAudioTracks())
        return stream
      }
      Reflect.set(window, "historyResourceState", () => ({
        sockets: sockets.length,
        openSockets: sockets.filter(
          (socket) => socket.readyState === NativeWebSocket.OPEN
        ).length,
        closedSockets: sockets.filter(
          (socket) => socket.readyState === NativeWebSocket.CLOSED
        ).length,
        liveMicrophones: tracks.filter((track) => track.readyState === "live")
          .length,
      }))
      // Deliver a queued SDK message even after its underlying browser socket has closed.
      Reflect.set(window, "historyLateResponse", () =>
        sockets[0]?.dispatchEvent(
          new MessageEvent("message", {
            data: JSON.stringify({
              type: "agent_response",
              agent_response_event: {
                agent_response: "Late deleted response",
                event_id: 999999,
                response_id: 999999,
              },
            }),
          })
        )
      )
    })
    const ordered: string[] = []
    page.on("request", (request) => {
      const path = new URL(request.url()).pathname
      if (path === "/api/voice/session" && request.method() === "DELETE")
        ordered.push("session-delete")
      if (
        path === "/api/voice/history" &&
        request.postDataJSON()?.command === "delete"
      )
        ordered.push("history-delete")
    })
    let atDeletion: Awaited<ReturnType<typeof resources>> | undefined
    await page.route("**/api/voice/history", async (route) => {
      if (route.request().postDataJSON().command === "delete")
        atDeletion = await resources()
      return route.fallback()
    })
    await page.goto("/")
    await page.locator("[data-orb]").click()
    await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
    await expect
      .poll(async () => (await resources()).liveMicrophones)
      .toBeGreaterThan(0)
    await expect.poll(resources).toMatchObject({ sockets: 1, openSockets: 1 })
    await openHistory(page)
    await page
      .getByRole("button", {
        name: target === "active" ? /Harvest planning/ : /Last week's delivery/,
      })
      .click()
    await page
      .locator("[data-history]")
      .getByRole("button", { name: "Delete conversation", exact: true })
      .click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Delete", exact: true })
      .click()
    await expect
      .poll(() => commands.some((c) => c.command === "delete"))
      .toBe(true)
    if (target === "other") {
      expect(ordered).toEqual(["history-delete"])
      expect(atDeletion).toMatchObject({ openSockets: 1 })
      expect(atDeletion!.liveMicrophones).toBeGreaterThan(0)
      await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
      expect(
        await page.evaluate(
          () =>
            JSON.parse(sessionStorage.getItem("flow.conversation") || "{}").id
        )
      ).toBe(first)
      agent.say("Current voice call still works")
      await expect
        .poll(() =>
          commands.some(
            (c) =>
              c.command === "events" &&
              JSON.stringify(c.events).includes(
                "Current voice call still works"
              )
          )
        )
        .toBe(true)
      return
    }
    expect(ordered).toEqual(["session-delete", "history-delete"])
    expect(atDeletion).toMatchObject({
      sockets: 1,
      openSockets: 0,
      closedSockets: 1,
      liveMicrophones: 0,
    })
    await expect(page.getByRole("button", { name: v.addFiles })).toHaveCount(0)
    await expect
      .poll(() =>
        page.evaluate(() => sessionStorage.getItem("flow.conversation"))
      )
      .toBeNull()
    const savedEvents = commands.filter((c) => c.command === "events").length
    await page.evaluate(() => Reflect.get(window, "historyLateResponse")())
    expect(await resources()).toMatchObject({
      sockets: 1,
      openSockets: 0,
      closedSockets: 1,
      liveMicrophones: 0,
    })
    expect(agent.connections).toBe(1)
    expect(commands.filter((c) => c.command === "events")).toHaveLength(
      savedEvents
    )
    await expect(
      page.getByText("Late deleted response", { exact: false })
    ).toHaveCount(0)
    expect(
      await page.evaluate(() => sessionStorage.getItem("flow.conversation"))
    ).toBeNull()
    const restoredReads = commands.filter(
      (c) => c.command === "get" || c.command === "messages"
    ).length
    await page.reload()
    await expect(
      page.getByRole("heading", { name: "Conversation history", exact: true })
    ).toBeVisible()
    expect(
      commands.filter((c) => c.command === "get" || c.command === "messages")
    ).toHaveLength(restoredReads)
    expect(agent.connections).toBe(1)
  })
}

test("history is available without a rollout setting and starts no call while browsing", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  const sessions = watch(page, "session")
  await page.goto("/")
  await page.getByRole("button", { name: s.showHeader, exact: true }).click()
  await page
    .getByRole("link", { name: "Conversation history", exact: true })
    .click()
  await expect(
    page.getByRole("heading", { name: "Conversation history", exact: true })
  ).toBeVisible()
  await expect(
    page.getByText("No saved conversations yet.", { exact: true })
  ).toBeVisible()
  expect(agent.connections).toBe(0)
  expect(sessions.filter((request) => request.method === "POST")).toHaveLength(
    0
  )
})
