import type { Page } from "@playwright/test"
import { expect, expectFits, emptyOverview, test } from "./fake-api"
import en from "../../../../apps/office/messages/en.json" with { type: "json" }
import nl from "../../../../apps/office/messages/nl.json" with { type: "json" }
import sw from "../../../../apps/office/messages/sw.json" with { type: "json" }

test.use({
  permissions: ["microphone"],
  launchOptions: {
    args: [
      "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream",
    ],
  },
})

async function nativeConversation(page: Page) {
  let emit: (event: unknown) => void = () => {
    throw new Error("Conversation not connected")
  }
  let onUserMessage = () => {}
  await page.route("**/api/voice/session", (route) => {
    const body = route.request().postDataJSON()
    if (route.request().method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })
    if (body.command === "heartbeat")
      return route.fulfill({ json: { status: "active" } })
    return route.fulfill({
      json: {
        sessionId: "interface-session",
        conversationId: "interface-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.route("**/api/voice/tools", (route) =>
    route.fulfill({ json: { status: "completed", data: [] } })
  )
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    emit = (event) => socket.send(JSON.stringify(event))
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      if (event.type === "conversation_initiation_client_data")
        emit({
          type: "conversation_initiation_metadata",
          conversation_initiation_metadata_event: {
            conversation_id: "interface-conversation",
            agent_output_audio_format: "pcm_16000",
            user_input_audio_format: "pcm_16000",
          },
        })
      if (event.type === "user_message") onUserMessage()
    })
  })
  return {
    emit: (event: unknown) => emit(event),
    onUserMessage: (callback: () => void) => {
      onUserMessage = callback
    },
    call: (operation = "products_list", id = "interface-tool") =>
      emit({
        type: "client_tool_call",
        client_tool_call: {
          tool_name: "execute_genesy_tool",
          tool_call_id: id,
          parameters: { operation_id: operation, arguments_json: "{}" },
          expects_response: true,
        },
      }),
    reply: (message = "Here is the requested information.") =>
      emit({
        type: "agent_response",
        agent_response_event: { agent_response: message, event_id: 20 },
      }),
  }
}

test("assistant opens as a full-screen overlay with plain transcripts and keyboard dismissal", async ({
  page,
  api,
}, testInfo) => {
  emptyOverview(api)
  await page.emulateMedia({ reducedMotion: "reduce" })
  await page.goto("/voice")
  const overlay = page.getByRole("dialog", {
    name: en.voice.title,
    exact: true,
  })
  await expect(overlay).toBeVisible()
  await expectFits(page)
  const box = await overlay.boundingBox()
  const viewport = page.viewportSize()!
  expect(box?.x).toBe(0)
  expect(box?.y).toBe(0)
  expect(box?.width).toBe(viewport.width)
  expect(box?.height).toBe(viewport.height)
  await expect(overlay.getByRole("log")).toBeVisible()
  await expect(overlay.locator("[data-voice-orb]")).toBeVisible()
  await expect(overlay.locator("[data-voice-orb]")).toHaveAttribute(
    "data-animate",
    "false"
  )
  // Reduced motion: no animated strands, the three still brand rings instead.
  await expect(page.locator("[data-motion]")).toHaveCount(0)
  await expect(overlay.locator("[data-voice-orb] > span")).toHaveCount(3)
  const orb = await overlay.locator("[data-voice-orb]").boundingBox()
  expect(Math.abs(orb!.x + orb!.width / 2 - viewport.width / 2)).toBeLessThan(1)
  await expect(
    overlay.locator('[data-slot="tile"], [data-slot="card"]')
  ).toHaveCount(0)
  await expectFits(page)
  await overlay.screenshot({
    path: testInfo.outputPath("assistant-overlay.png"),
    animations: "disabled",
  })
  await page.keyboard.press("Escape")
  await expect(page).toHaveURL(/\/$/)
})

for (const textOnly of [true, false]) {
  test(`native ${textOnly ? "text" : "voice"} input stays visibly pending through delayed tool execution`, async ({
    page,
    api: _api,
  }, testInfo) => {
    void _api
    const native = await nativeConversation(page)
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    await page.route("**/api/voice/tools", async (route) => {
      if (route.request().postDataJSON().command === "status")
        return route.fulfill({ json: { status: "completed", data: [] } })
      await gate
      return route.fulfill({
        json: {
          status: "completed",
          data: { name: "Cocoa", quantity: "2.00" },
        },
      })
    })
    await page.goto("/voice")
    await page
      .getByRole("button", {
        name: textOnly ? en.voice.textInstead : en.voice.start,
        exact: true,
      })
      .click()
    // The composer is for text mode; voice mode keeps the camera for references.
    await expect(page.getByLabel(en.voice.message)).toHaveCount(
      textOnly ? 1 : 0
    )
    await expect(
      page.getByRole("button", { name: en.voice.takePhoto, exact: true })
    ).toBeVisible()
    if (textOnly) {
      await page.getByLabel(en.voice.message).fill("Check stock")
      await page
        .getByRole("button", { name: en.voice.send, exact: true })
        .click()
    } else
      native.emit({
        type: "user_transcript",
        user_transcription_event: {
          user_transcript: "Check stock",
          event_id: 5,
        },
      })
    const activity = page.locator("[data-voice-activity]")
    const thinking = page.getByRole("log").locator("[data-voice-thinking]")
    await expect(activity).toHaveAttribute("data-state", "thinking")
    await expect(activity).toHaveAttribute("aria-busy", "true")
    // While pending, the thinking line in the chat shows it; the status stays for screen readers.
    await expect(activity).toHaveClass(/\bsr-only\b/)
    await expect(thinking).toBeVisible()
    await expect(thinking).toContainText(en.voice.activity.thinking)
    if (textOnly)
      await page.screenshot({
        path: testInfo.outputPath("assistant-thinking.png"),
        animations: "disabled",
      })
    native.call()
    await expect(activity).toHaveAttribute("data-state", "executing")
    await expect(thinking).toContainText(en.voice.activity.executing)
    native.reply("I am checking the stock.")
    await expect(activity).toHaveAttribute("data-state", "executing")
    release()
    await expect(activity).toHaveAttribute("data-state", "completed")
    await expect(thinking).toHaveCount(0)
    await expect(activity).not.toHaveClass(/\bsr-only\b/)
    await expect(
      page.getByRole("region", { name: en.voice.currentTask, exact: true })
    ).toContainText("Cocoa")
    await expectFits(page)
    await page.getByRole("button", { name: en.voice.end, exact: true }).click()
  })
}

test.describe("native request failure", () => {
  test.use({ allow: { responses: [{ url: "/api/voice/tools", status: 500 }] } })
  test("shows a visible error and clears loading without claiming completion", async ({
    page,
    api: _api,
  }) => {
    void _api
    const native = await nativeConversation(page)
    await page.route("**/api/voice/tools", (route) =>
      route.request().postDataJSON().command === "status"
        ? route.fulfill({ json: { status: "completed", data: [] } })
        : route.fulfill({
            status: 500,
            json: { message: "Internal provider diagnostic" },
          })
    )
    await page.goto("/voice")
    await page
      .getByRole("button", { name: en.voice.textInstead, exact: true })
      .click()
    await expect(page.getByLabel(en.voice.message)).toBeVisible()
    native.call()
    await expect(page.locator("[data-voice-activity]")).toHaveAttribute(
      "data-state",
      "failed"
    )
    await expect(page.locator("[data-voice-activity]")).toHaveAttribute(
      "aria-busy",
      "false"
    )
    await expect(page.getByRole("alert")).toBeVisible()
    await expect(page.getByText("Internal provider diagnostic")).toHaveCount(0)
    native.onUserMessage(() => native.reply("We can continue."))
    await page.getByLabel(en.voice.message).fill("Continue with a new request")
    await page.getByRole("button", { name: en.voice.send, exact: true }).click()
    await expect(page.locator("[data-voice-activity]")).toHaveAttribute(
      "data-state",
      "ready"
    )
    native.call("products_list", "second-failure")
    await expect(page.locator("[data-voice-activity]")).toHaveAttribute(
      "data-state",
      "failed"
    )
    native.emit({
      type: "user_transcript",
      user_transcription_event: {
        user_transcript: "A fresh request",
        event_id: 7,
      },
    })
    await expect(page.locator("[data-voice-activity]")).toHaveAttribute(
      "data-state",
      "thinking"
    )
    native.reply("Ready to continue.")
    await expect(page.locator("[data-voice-activity]")).toHaveAttribute(
      "data-state",
      "ready"
    )
    await expectFits(page)
  })
})

test("invalid tool arguments show a failure without starting a backend request", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page)
  let prepares = 0
  await page.route("**/api/voice/tools", (route) => {
    if (route.request().postDataJSON().command === "execute") prepares++
    return route.fulfill({ json: { status: "completed", data: [] } })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  native.emit({
    type: "client_tool_call",
    client_tool_call: {
      tool_name: "execute_genesy_tool",
      tool_call_id: "invalid",
      parameters: { operation_id: "orders_create", arguments_json: "{" },
      expects_response: true,
    },
  })
  await expect(page.locator("[data-voice-activity]")).toHaveAttribute(
    "data-state",
    "failed"
  )
  await expect(page.getByRole("alert")).toContainText(en.voice.requestFailed)
  expect(prepares).toBe(0)
})

for (const [locale, messages] of Object.entries({ en, nl, sw })) {
  test(`${locale} approval slides in with readable references and localized fields`, async ({
    page,
    api: _api,
    baseURL,
  }, testInfo) => {
    void _api
    const native = await nativeConversation(page)
    await page
      .context()
      .addCookies([{ name: "NEXT_LOCALE", value: locale, url: baseURL! }])
    const id = "01950000-0000-7000-8000-000000000001"
    let approvals = 0
    await page.route("**/api/voice/tools", (route) =>
      route.request().postDataJSON().command === "status"
        ? route.fulfill({ json: { status: "completed", data: [] } })
        : route.fulfill({
            json: {
              status: "requires_confirmation",
              data: { actionId: "internal-action" },
              review: {
                ticket: "private-review-ticket",
                operationId: "products_update",
                method: "PATCH",
                summary: "Update product",
                target: "products",
                details: {
                  workspace: "01950000-0000-7000-8000-000000000009",
                  target: { product: id },
                  record: { id, name: "Cocoa", reference: "COC-01" },
                  changes: {
                    quantity: "2.00",
                    total: "€1.25",
                    recipient: "https://supplier.example.test/orders",
                  },
                },
                confirmationCode: "123456",
                confirmationPhrase: "Confirm action 123456",
                expiresAt: new Date(Date.now() + 120000).toISOString(),
              },
            },
          })
    )
    await page.route("**/api/voice/confirm", async (route) => {
      const body = route.request().postDataJSON()
      if (body.decision === "approve") approvals++
      return route.fulfill({
        json: { status: body.decision === "approve" ? "queued" : "cancelled" },
      })
    })
    await page.goto("/voice")
    await page
      .getByRole("button", { name: messages.voice.textInstead, exact: true })
      .click()
    await expect(page.getByLabel(messages.voice.message)).toBeVisible()
    native.call("products_update")
    const review = page.getByRole("dialog", {
      name: messages.voice.review,
      exact: true,
    })
    await expect(review).toBeVisible()
    await expect(review).toHaveAttribute("data-side", "right")
    // One row per reviewed field, in the order the server sent them, none hidden.
    const rows = review.locator('.voice-review-list [role="option"]')
    await expect(rows).toHaveCount(4)
    for (let i = 0; i < 4; i++) await expect(rows.nth(i)).toBeVisible()
    await expect(rows.nth(2)).toContainText("Cocoa")
    await expect(rows.nth(3)).toContainText("2.00")
    await expect(review).toContainText("Cocoa")
    await expect(review).toContainText("COC-01")
    await expect(review).toContainText("2.00")
    await expect(review).toContainText("€1.25")
    await expect(
      review.getByRole("link", {
        name: "https://supplier.example.test/orders",
        exact: true,
      })
    ).toBeVisible()
    await expect(review).toContainText(messages.voice.reviewFields.quantity)
    await expect(review).not.toContainText(id)
    await expect(review).not.toContainText(
      "01950000-0000-7000-8000-000000000009"
    )
    await expect(review).not.toContainText("private-review-ticket")
    if (locale !== "en")
      await expect(review).not.toContainText("Update product")
    expect(approvals).toBe(0)
    await expectFits(page)
    if (locale === "en")
      await review.screenshot({
        path: testInfo.outputPath("assistant-approval.png"),
        animations: "disabled",
      })
    await review
      .getByRole("button", { name: messages.voice.approve, exact: true })
      .click()
    await expect(review).toBeHidden()
    await expect(page.locator("[data-voice-activity]")).toHaveAttribute(
      "data-state",
      "queued"
    )
    expect(approvals).toBe(1)
    await expectFits(page)
  })
}

test("text composer has only the message box, Attach and Send, and chat bubbles sit by role", async ({
  page,
  api,
}) => {
  emptyOverview(api)
  const native = await nativeConversation(page)
  native.onUserMessage(() => native.reply("Here is the stock."))
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  const box = page.getByRole("textbox", { name: en.voice.message, exact: true })
  await expect(box).toBeVisible()
  for (const name of ["Choose model", "Choose effort", "Dictate"])
    await expect(page.getByRole("button", { name, exact: true })).toHaveCount(0)
  await page
    .getByRole("button", { name: en.voice.addFiles, exact: true })
    .click()
  const menu = page.getByRole("listbox", {
    name: en.voice.addFiles,
    exact: true,
  })
  await expect(menu.getByRole("option")).toHaveCount(1)
  await expect(menu.getByRole("option")).toContainText(en.voice.attach)
  await page.keyboard.press("Escape")
  await expect(menu).toHaveCount(0)
  await expectFits(page)
  await box.fill("Check stock")
  await page.getByRole("button", { name: en.voice.send, exact: true }).click()
  const log = page.getByRole("log")
  const user = log.locator('[data-role="user"]').last()
  const agent = log.locator('[data-role="agent"]').last()
  await expect(user).toContainText("Check stock")
  await expect(agent).toContainText("Here is the stock.")
  await expect(box).toHaveValue("")
  const area = (await log.boundingBox())!
  const mine = (await user.boundingBox())!
  const theirs = (await agent.boundingBox())!
  // User on the right, assistant on the left (4 px log padding, no scrollbar room needed).
  expect(area.x + area.width - (mine.x + mine.width)).toBeLessThan(12)
  expect(mine.x - area.x).toBeGreaterThan(12)
  expect(theirs.x - area.x).toBeLessThan(12)
  await expectFits(page)
  await page.getByRole("button", { name: en.voice.end, exact: true }).click()
})

test("with motion allowed the avatar is the bare animated strands, without the still rings", async ({
  page,
  api: _api,
}) => {
  void _api
  await page.goto("/voice")
  const orb = page.locator("[data-voice-orb]")
  await expect(orb.locator("[data-motion]")).toHaveCount(1)
  await expect(orb.locator(":scope > span:not([data-motion])")).toHaveCount(0)
  await expect(orb).toHaveCSS("background-image", "none")
})

// shortcut: needs sibling session C (tool responses carry `steps`, provider exposes them as
// `voice.steps`); turn on once C is merged.
test.fixme("thinking line lists the task steps while an approved task runs", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page)
  await page.route("**/api/voice/tools", (route) =>
    route.request().postDataJSON().command === "status"
      ? route.fulfill({ json: { status: "completed", data: [] } })
      : route.fulfill({
          json: {
            status: "requires_confirmation",
            data: { actionId: "steps-action" },
            steps: ["Checking: products", "Preparing your review"],
            review: {
              ticket: "steps-ticket",
              operationId: "products_update",
              method: "PATCH",
              summary: "Update product",
              details: { quantity: "2.00" },
              confirmationCode: "135790",
              confirmationPhrase: "Confirm action 135790",
              expiresAt: new Date(Date.now() + 120000).toISOString(),
            },
          },
        })
  )
  let release!: () => void
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  await page.route("**/api/voice/confirm", async (route) => {
    await gate
    return route.fulfill({
      json: { status: "completed", steps: ["Running it now", "Finalizing"] },
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  native.call("products_update")
  const review = page.getByRole("dialog", {
    name: en.voice.review,
    exact: true,
  })
  await review
    .getByRole("button", { name: en.voice.approve, exact: true })
    .click()
  const thinking = page.getByRole("log").locator("[data-voice-thinking]")
  await expect(thinking).toContainText("Running it now")
  release()
  await expect(thinking).toHaveCount(0)
})

test("the design preview route is not served by the production build", async ({
  page,
  api: _api,
}) => {
  void _api
  // The app shell streams first, so the status stays 200; the page itself is "not found".
  await page.goto("/voice/design")
  await expect(page.getByText(en.states.pageNotFound, { exact: true })).toBeVisible()
  await expect(page.locator("[data-design-switcher]")).toHaveCount(0)
  await expect(page.getByRole("dialog", { name: en.voice.title })).toHaveCount(
    0
  )
})
