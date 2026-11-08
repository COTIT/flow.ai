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

async function nativeConversation(page: Page, timeZone?: string) {
  let emit: (event: unknown) => void = () => {
    throw new Error("Conversation not connected")
  }
  let onUserMessage = () => {}
  const sent: { type?: string; [key: string]: unknown }[] = []
  /** Session commands the page sent (start, heartbeat, dropped). */
  const sessions: { command?: string; [key: string]: unknown }[] = []
  await page.route("**/api/voice/session", (route) => {
    const body = route.request().postDataJSON()
    if (route.request().method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })
    sessions.push(body)
    if (body.command === "heartbeat")
      return route.fulfill({ json: { status: "active" } })
    if (body.command === "dropped")
      return route.fulfill({ json: { status: "recorded" } })
    return route.fulfill({
      json: {
        sessionId: "interface-session",
        conversationId: "interface-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
        ...(timeZone ? { timeZone } : {}),
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
      sent.push(event)
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
    /** Everything the page sent to the agent. */
    sent,
    sessions,
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
    reply: (message = "Here is the requested information.", eventId = 20) =>
      emit({
        type: "agent_response",
        agent_response_event: { agent_response: message, event_id: eventId },
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
    await expect(thinking).toBeInViewport()
    // The same "checking" line again before a second tool call shows once.
    native.reply("I am checking the stock.", 21)
    release()
    // The tool answered; the agent still owes its reply, so the row stays.
    await expect(activity).toHaveAttribute("data-state", "thinking")
    await expect(thinking).toBeInViewport()
    native.reply("Cocoa has 2.00 in stock.", 22)
    await expect(activity).toHaveAttribute("data-state", "completed")
    await expect(thinking).toHaveCount(0)
    await expect(activity).not.toHaveClass(/\bsr-only\b/)
    await expect(
      page.getByRole("log").getByText("I am checking the stock.")
    ).toHaveCount(1)
    // No result log under the chat: the agent's reply says it.
    await expect(page.locator("#voice-task")).toHaveCount(0)
    await expect(page.getByText("2.00", { exact: true })).toHaveCount(0)
    await expect(page.getByText(en.voice.recoveryTitle)).toHaveCount(0)
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
  await expect(
    page.getByText(en.states.pageNotFound, { exact: true })
  ).toBeVisible()
  await expect(page.locator("[data-design-switcher]")).toHaveCount(0)
  await expect(page.getByRole("dialog", { name: en.voice.title })).toHaveCount(
    0
  )
})

test("a long review keeps every row reachable by scrolling the sheet, none hidden in the list", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page)
  const details = Object.fromEntries(
    Array.from({ length: 12 }, (_, i) => [`field_${i + 1}`, `Value ${i + 1}`])
  )
  await page.route("**/api/voice/tools", (route) =>
    route.request().postDataJSON().command === "status"
      ? route.fulfill({ json: { status: "completed", data: [] } })
      : route.fulfill({
          json: {
            status: "requires_confirmation",
            data: { actionId: "long-action" },
            review: {
              ticket: "long-ticket",
              operationId: "products_update",
              method: "PATCH",
              summary: "Update product",
              details,
              confirmationCode: "112233",
              confirmationPhrase: "Confirm action 112233",
              expiresAt: new Date(Date.now() + 120000).toISOString(),
            },
          },
        })
  )
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  native.call("products_update")
  const review = page.getByRole("dialog", {
    name: en.voice.review,
    exact: true,
  })
  const rows = review.locator('.voice-review-list [role="option"]')
  await expect(rows).toHaveCount(12)
  // The list has no scrollbar of its own, so it must not clip rows; the sheet scrolls instead.
  const list = review.locator('.voice-review-list [role="listbox"]')
  expect(
    await list.evaluate((el) => el.scrollHeight - el.clientHeight)
  ).toBeLessThanOrEqual(1)
  // A real wheel over the rows scrolls the sheet to the last row and the approval button.
  await expect(rows.nth(11)).toHaveAttribute("data-state", /.+/)
  const first = await rows.first().boundingBox()
  await page.mouse.move(
    first!.x + first!.width / 2,
    first!.y + first!.height / 2
  )
  for (let i = 0; i < 10; i++) await page.mouse.wheel(0, 400)
  await expect(rows.nth(11)).toBeInViewport()
  await expect(
    review.getByRole("button", { name: en.voice.approve, exact: true })
  ).toBeInViewport()
  for (let i = 0; i < 12; i++) {
    const row = rows.nth(i)
    await row.scrollIntoViewIfNeeded()
    await expect(row).toBeInViewport()
    await expect(row).toContainText(`Value ${i + 1}`)
    // Revealed, not left at the list's hidden pose.
    await expect
      .poll(() =>
        row.evaluate((el) => {
          let shown = 1
          for (let node: Element | null = el; node; node = node.parentElement) {
            shown *= Number(getComputedStyle(node).opacity)
            if (node.getAttribute("role") === "listbox") break
          }
          return shown
        })
      )
      .toBe(1)
  }
  await expect(
    review.getByRole("button", { name: en.voice.approve, exact: true })
  ).toBeInViewport()
  await expectFits(page)
})

test("a long chat keeps the avatar, status, chat and composer apart, and thinking steps wrap", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page)
  let release!: () => void
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  await page.route("**/api/voice/tools", async (route) => {
    await gate
    return route.fulfill({
      json: {
        status: "completed",
        data: { name: "Cocoa" },
        // Step keys become short localized labels; catalog text never shows.
        steps: ["checking", "preparing"],
      },
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  for (let i = 0; i < 8; i++)
    native.reply(
      `Answer ${i}: a fairly long sentence about stock in Asheville warehouse, AFM store and the market stall.`,
      100 + i
    )
  native.call()
  const box = async (selector: string) =>
    (await page.locator(selector).first().boundingBox())!
  const apart = (a: { y: number; height: number }, b: { y: number }) =>
    expect(a.y + a.height).toBeLessThanOrEqual(b.y + 1)
  const orb = await box("[data-voice-orb]")
  const log = await box('[role="log"]')
  const composer = await box(".voice-composer")
  apart(orb, log)
  apart(log, composer)
  const thinking = page.locator("[data-voice-thinking]")
  await expect(thinking).toBeVisible()
  release()
  await expect(thinking).toContainText(en.voice.steps.checking)
  expect(
    await thinking.evaluate((el) => {
      const outer = el.getBoundingClientRect()
      return [...el.querySelectorAll("*")].every((child) => {
        const r = child.getBoundingClientRect()
        return !r.width || r.right <= outer.right + 1
      })
    })
  ).toBe(true)
  await expectFits(page)
})

test("voice starts in push-to-talk: the mic opens only while held, and open mic is remembered", async ({
  page,
  api: _api,
}) => {
  void _api
  await nativeConversation(page)
  await page.goto("/voice")
  await page.getByRole("button", { name: en.voice.start, exact: true }).click()
  const hold = page.getByRole("button", {
    name: en.voice.holdToTalk,
    exact: true,
  })
  const activity = page.locator("[data-voice-activity]")
  await expect(hold).toBeVisible()
  await expect(hold).toHaveAttribute("aria-pressed", "false")
  await expect(activity).toHaveAttribute("data-state", "holdToTalk")
  await expect(page.getByRole("button", { name: en.voice.mute })).toHaveCount(0)
  const size = (await hold.boundingBox())!
  expect(size.height).toBeGreaterThanOrEqual(44)
  // Pointer hold opens the mic; release closes it again.
  await hold.hover()
  await page.mouse.down()
  await expect(hold).toHaveAttribute("aria-pressed", "true")
  await expect(activity).toHaveAttribute("data-state", "listening")
  await page.mouse.up()
  await expect(hold).toHaveAttribute("aria-pressed", "false")
  await expect(activity).toHaveAttribute("data-state", "holdToTalk")
  // Space works from anywhere outside text fields.
  await page.locator("body").focus()
  await page.keyboard.down("Space")
  await expect(activity).toHaveAttribute("data-state", "listening")
  await page.keyboard.up("Space")
  await expect(activity).toHaveAttribute("data-state", "holdToTalk")
  await expectFits(page)
  await page
    .getByRole("button", { name: en.voice.openMic, exact: true })
    .click()
  await expect(hold).toHaveCount(0)
  await expect(activity).toHaveAttribute("data-state", "listening")
  await expect(
    page.getByRole("button", { name: en.voice.mute, exact: true })
  ).toBeVisible()
  await page.getByRole("button", { name: en.voice.end, exact: true }).click()
  await page.reload()
  await page.getByRole("button", { name: en.voice.start, exact: true }).click()
  await expect(
    page.getByRole("button", { name: en.voice.pushToTalk, exact: true })
  ).toBeVisible()
  await expect(hold).toHaveCount(0)
})

test("a created record shows as a link that opens in a new tab, and the agent hears only its name", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page)
  const order = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
  await page.route("**/api/voice/tools", (route) =>
    route.fulfill({
      json: {
        status: "completed",
        data: { id: order, number: "ORD-0229" },
        links: [{ label: "ORD-0229", href: `/orders/${order}` }],
      },
    })
  )
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  native.call("orders_create")
  const link = page
    .getByRole("log")
    .getByRole("link", { name: "Open ORD-0229 in a new tab", exact: true })
  await expect(link).toBeVisible()
  await expect(link).toHaveAttribute("target", "_blank")
  await expect(link).toHaveAttribute("rel", /noopener/)
  expect((await link.boundingBox())!.height).toBeGreaterThanOrEqual(44)
  const result = native.sent.find(
    (event) => event.type === "client_tool_result"
  )
  expect(JSON.stringify(result)).toContain("ORD-0229")
  expect(JSON.stringify(result)).not.toContain("/orders/")
  await expectFits(page)
  const [tab] = await Promise.all([
    page.context().waitForEvent("page"),
    link.click(),
  ])
  await tab.waitForLoadState()
  // One login per browser: the new tab opens the order, not the sign-in page.
  await expect(tab).toHaveURL(new RegExp(`/orders/${order}$`))
  await tab.close()
})

test("a refused request shows a plain reason on screen and gives the agent the reason to explain", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page)
  await page.route("**/api/voice/tools", (route) =>
    route.fulfill({
      json: {
        status: "failed",
        reason: "missing",
        httpStatus: 404,
        message: "Order not found.",
      },
    })
  )
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  native.call("orders_get")
  await expect(page.getByRole("alert")).toHaveText(
    en.voice.requestErrors.missing
  )
  await expect(page.getByText("404")).toHaveCount(0)
  await expect
    .poll(() =>
      JSON.stringify(
        native.sent.find((event) => event.type === "client_tool_result")
      )
    )
    .toContain('\\"reason\\":\\"missing\\"')
  await expectFits(page)
})

const userMessages = (sent: { type?: string; text?: unknown }[]) =>
  sent.filter((event) => event.type === "user_message").map((e) => e.text)

test("a message typed while the agent is still answering waits for that answer, and steps show plain labels", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page, "Africa/Dar_es_Salaam")
  let release!: () => void
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  await page.route("**/api/voice/tools", async (route) => {
    await gate
    return route.fulfill({
      json: {
        status: "completed",
        data: [],
        // A key the page does not know is dropped, never shown raw.
        steps: [
          "checking",
          "List invoices (a page, or `limit` rows without `meta`)",
        ],
      },
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  // The agent is told the workspace zone, so it never guesses the device's.
  await expect
    .poll(() =>
      JSON.stringify(
        native.sent.filter((event) => event.type === "contextual_update")
      )
    )
    .toContain("Africa/Dar_es_Salaam")
  await page
    .getByLabel(en.voice.message)
    .fill("Which products are out of stock?")
  await page.getByRole("button", { name: en.voice.send, exact: true }).click()
  // Before the agent starts, a quick follow-up still goes straight out and merges into the request.
  await page.getByLabel(en.voice.message).fill("Or below zero.")
  await page.getByRole("button", { name: en.voice.send, exact: true }).click()
  await expect
    .poll(() => userMessages(native.sent))
    .toEqual(["Which products are out of stock?", "Or below zero."])
  native.reply("I'm checking negative stock.")
  native.call("stock_list")
  const thinking = page.getByRole("log").locator("[data-voice-thinking]")
  await expect(thinking).toBeVisible()
  // The second question must not reach the agent while it still owes the first answer.
  await page.getByLabel(en.voice.message).fill("And yesterday's till total?")
  await page.getByRole("button", { name: en.voice.send, exact: true }).click()
  await expect(
    page.getByRole("log").getByText("And yesterday's till total?")
  ).toBeVisible()
  release()
  await expect(thinking).toContainText(en.voice.steps.checking)
  await expect(page.getByText("List invoices", { exact: false })).toHaveCount(0)
  expect(userMessages(native.sent)).toEqual([
    "Which products are out of stock?",
    "Or below zero.",
  ])
  native.reply("No products are below zero.", 21)
  // After the answer settles, the held question goes out on its own.
  await expect
    .poll(() => userMessages(native.sent))
    .toEqual([
      "Which products are out of stock?",
      "Or below zero.",
      "And yesterday's till total?",
    ])
  await expect(
    page.getByRole("log").getByText("And yesterday's till total?")
  ).toHaveCount(1)
  native.reply("The till took 120,000 TZS yesterday.", 22)
  await expect(page.locator("[data-voice-activity]")).not.toHaveAttribute(
    "data-state",
    "thinking"
  )
  await expectFits(page)
})

test("an agent that goes quiet mid-request ends with a plain 'couldn't finish' line and Ask again", async ({
  page,
  api: _api,
}) => {
  void _api
  await page.clock.install()
  const native = await nativeConversation(page)
  await page.route("**/api/voice/tools", (route) =>
    route.fulfill({ json: { status: "completed", data: [] } })
  )
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  await page.getByLabel(en.voice.message).fill("Compare the till with Friday")
  await page.getByRole("button", { name: en.voice.send, exact: true }).click()
  native.reply("I'm checking yesterday's sales.")
  native.call("till_sales_list")
  await expect
    .poll(() => native.sent.some((e) => e.type === "client_tool_result"))
    .toBe(true)
  await expect(page.locator("[data-voice-activity]")).toHaveAttribute(
    "data-state",
    "thinking"
  )
  // The agent never answers after its tool result.
  await page.clock.fastForward(46000)
  const line = page
    .getByRole("log")
    .getByText(en.voice.unfinished.stalled, { exact: false })
  await expect(line).toBeVisible()
  await expect(line).toContainText("1 step was finished.")
  await expect(
    page.getByRole("log").locator("[data-voice-thinking]")
  ).toHaveCount(0)
  const again = page.getByRole("button", {
    name: en.voice.unfinished.askAgain,
    exact: true,
  })
  await expect(again).toBeVisible()
  await expectFits(page)
  await again.click()
  await expect
    .poll(() => userMessages(native.sent))
    .toEqual(["Compare the till with Friday", "Compare the till with Friday"])
  await expect(again).toHaveCount(0)
})

test("the provider ending the call mid-request is a failure, never Completed, and Continue opens a new conversation", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page)
  await page.route("**/api/voice/tools", (route) =>
    route.fulfill({ json: { status: "completed", data: [] } })
  )
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  await page.getByLabel(en.voice.message).fill("Create an order for Mama Grace")
  await page.getByRole("button", { name: en.voice.send, exact: true }).click()
  native.reply("I'm finding the customer and products.")
  native.call("relations_list")
  // One lookup finished and its result reached the agent before the call ended.
  await expect
    .poll(() => native.sent.some((e) => e.type === "client_tool_result"))
    .toBe(true)
  native.emit({
    type: "error",
    error_event: {
      error_type: "max_duration_exceeded",
      message: "Maximum conversation duration exceeded",
    },
  })
  const activity = page.locator("[data-voice-activity]")
  await expect(activity).toHaveAttribute("data-state", "failed")
  await expect(
    page.getByRole("log").getByText(en.voice.unfinished.time_limit, {
      exact: false,
    })
  ).toContainText("1 step was finished.")
  await expect(page.getByText(en.voice.activity.completed)).toHaveCount(0)
  await expect
    .poll(() => native.sessions.filter((s) => s.command === "dropped"))
    .toEqual([
      expect.objectContaining({
        endedBy: "time_limit",
        sessionId: "interface-session",
      }),
    ])
  await expectFits(page)
  await page
    .getByRole("button", { name: en.voice.unfinished.continue, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  await expect(
    page.getByRole("log").getByText(en.voice.unfinished.continued)
  ).toBeVisible()
  // Earlier messages stay; nothing was sent again on the agent's behalf.
  await expect(
    page.getByRole("log").getByText("Create an order for Mama Grace")
  ).toBeVisible()
  await expect(
    page.getByRole("button", {
      name: en.voice.unfinished.continue,
      exact: true,
    })
  ).toHaveCount(0)
})

test("an open review card survives a resized window, focus loss and a stray click; only an explicit action cancels it", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page)
  const decisions: string[] = []
  await page.route("**/api/voice/tools", (route) =>
    route.request().postDataJSON().command === "status"
      ? route.fulfill({ json: { status: "completed", data: [] } })
      : route.fulfill({
          json: {
            status: "requires_confirmation",
            data: { actionId: "internal-action" },
            review: {
              ticket: "private-review-ticket",
              operationId: "invoices_credit",
              method: "POST",
              summary: "Credit invoice 17",
              details: { invoice: "17" },
              confirmationCode: "123456",
              confirmationPhrase: "Confirm action 123456",
              expiresAt: new Date(Date.now() + 120000).toISOString(),
            },
          },
        })
  )
  await page.route("**/api/voice/confirm", (route) => {
    decisions.push(route.request().postDataJSON().decision)
    return route.fulfill({ json: { status: "cancelled" } })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  native.call("invoices_credit")
  const card = page.getByRole("dialog", { name: en.voice.review })
  await expect(card).toBeVisible()
  const size = page.viewportSize()!
  await page.setViewportSize({ width: size.width - 40, height: size.height })
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"))
    ;(document.activeElement as HTMLElement | null)?.blur()
  })
  await page.mouse.click(5, 5)
  await page.setViewportSize(size)
  await expect(card).toBeVisible()
  expect(decisions).toEqual([])
  await page.keyboard.press("Escape")
  await expect(card).toHaveCount(0)
  await expect.poll(() => decisions).toEqual(["cancel"])
})

test("while a review card is open the message box cannot be reached by pointer, keyboard or Tab", async ({
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
            data: { actionId: "internal-action" },
            review: {
              ticket: "private-review-ticket",
              operationId: "invoices_credit",
              method: "POST",
              summary: "Credit invoice 17",
              details: { invoice: "17" },
              confirmationCode: "123456",
              confirmationPhrase: "Confirm action 123456",
              expiresAt: new Date(Date.now() + 120000).toISOString(),
            },
          },
        })
  )
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  const box = page.getByLabel(en.voice.message)
  await expect(box).toBeVisible()
  native.call("invoices_credit")
  await expect(
    page.getByRole("dialog", { name: en.voice.review })
  ).toBeVisible()
  const focused = () =>
    box.evaluate(
      (el) =>
        el === document.activeElement || el.contains(document.activeElement)
    )
  // Pointer: whatever sits on top of the box, it is not the box.
  const rect = (await box.boundingBox())!
  expect(
    await page.evaluate(
      ([x, y]) => {
        const hit = document.elementFromPoint(x!, y!)
        return Boolean(hit?.closest('[aria-label="Message the assistant"]'))
      },
      [rect.x + rect.width / 2, rect.y + rect.height / 2]
    )
  ).toBe(false)
  await page.mouse.click(rect.x + rect.width / 2, rect.y + rect.height / 2)
  expect(await focused()).toBe(false)
  // Script focus and Tab cycling never land in it either.
  await box.evaluate((el) => (el as HTMLElement).focus())
  expect(await focused()).toBe(false)
  for (let i = 0; i < 15; i++) {
    await page.keyboard.press("Tab")
    expect(await focused()).toBe(false)
  }
  await page.keyboard.type("cancel it")
  await page.keyboard.press("Enter")
  expect(native.sent.filter((event) => event.type === "user_message")).toEqual(
    []
  )
  await expect(
    page.getByRole("dialog", { name: en.voice.review })
  ).toBeVisible()
})

test("a review cancelled because the page was hidden says so in the chat and offers to prepare it again", async ({
  page,
  api: _api,
}) => {
  void _api
  const native = await nativeConversation(page)
  const decisions: string[] = []
  await page.route("**/api/voice/tools", (route) =>
    route.request().postDataJSON().command === "status"
      ? route.fulfill({ json: { status: "completed", data: [] } })
      : route.fulfill({
          json: {
            status: "requires_confirmation",
            data: { actionId: "internal-action" },
            review: {
              ticket: "private-review-ticket",
              operationId: "invoices_credit",
              method: "POST",
              summary: "Credit invoice 17",
              details: { invoice: "17" },
              confirmationCode: "123456",
              confirmationPhrase: "Confirm action 123456",
              expiresAt: new Date(Date.now() + 120000).toISOString(),
            },
          },
        })
  )
  await page.route("**/api/voice/confirm", (route) => {
    decisions.push(route.request().postDataJSON().decision)
    return route.fulfill({ json: { status: "cancelled" } })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  native.call("invoices_credit")
  await expect(
    page.getByRole("dialog", { name: en.voice.review })
  ).toBeVisible()
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", {
      value: "hidden",
      configurable: true,
    })
    document.dispatchEvent(new Event("visibilitychange"))
  })
  await expect.poll(() => decisions).toEqual(["cancel"])
  await expect(
    page.getByRole("log").getByText(en.voice.hiddenCancelled)
  ).toBeVisible()
  await expectFits(page)
})
