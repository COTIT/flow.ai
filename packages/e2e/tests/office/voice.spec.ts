import { expect, expectFits, emptyOverview, openMenu, test } from "./fake-api"
import en from "../../../../apps/office/messages/en.json" with { type: "json" }
import nl from "../../../../apps/office/messages/nl.json" with { type: "json" }
import sw from "../../../../apps/office/messages/sw.json" with { type: "json" }

function referencePdf() {
  const content = "BT /F1 12 Tf 40 100 Td (Customer reference ABC) Tj ET"
  const objects = [
    "<</Type/Catalog/Pages 2 0 R>>",
    "<</Type/Pages/Kids[3 0 R]/Count 1>>",
    "<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]/Resources<</Font<</F1 5 0 R>>>>/Contents 4 0 R>>",
    `<</Length ${content.length}>>\nstream\n${content}\nendstream`,
    "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>",
  ]
  let pdf = "%PDF-1.7\n"
  const offsets = [0]
  objects.forEach((object, index) => {
    offsets.push(Buffer.byteLength(pdf))
    pdf += `${index + 1} 0 obj\n${object}\nendobj\n`
  })
  const xref = Buffer.byteLength(pdf)
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n${offsets
    .slice(1)
    .map((offset) => `${String(offset).padStart(10, "0")} 00000 n \n`)
    .join(
      ""
    )}trailer\n<</Size ${objects.length + 1}/Root 1 0 R>>\nstartxref\n${xref}\n%%EOF\n`
  return Buffer.from(pdf)
}

test.beforeEach(async ({ page }) => {
  await page.route("**/api/voice/tools", (route) => {
    if (
      route.request().headers()["content-type"] === "application/json" &&
      route.request().postDataJSON()?.command === "status"
    )
      return route.fulfill({ json: { status: "completed", data: [] } })
    return route.fallback()
  })
})

test.use({
  permissions: ["microphone"],
  launchOptions: {
    args: [
      "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream",
    ],
  },
})

for (const scenario of [
  {
    locale: "nl",
    messages: nl,
    status: 409,
    expected:
      "Dit voorstel is gewijzigd of verlopen. Bekijk de status in Office en vraag zo nodig om een nieuw voorstel.",
  },
  {
    locale: "sw",
    messages: sw,
    status: 403,
    expected:
      "Huna ruhusa ya kufanya kitendo hiki. Angalia ufikiaji wako katika Office.",
  },
  {
    locale: "nl",
    messages: nl,
    status: 501,
    expected:
      "Deze functie is in deze versie niet beschikbaar. Er is geen actie gestart.",
  },
]) {
  test.describe(`${scenario.locale} request failure ${scenario.status}`, () => {
    test.use({
      allow: {
        responses: [{ url: "/api/voice/session", status: scenario.status }],
      },
    })
    test("shows localized recovery guidance instead of raw server details", async ({
      page,
      api: _api,
      baseURL,
    }) => {
      void _api
      await page
        .context()
        .addCookies([
          { name: "NEXT_LOCALE", value: scenario.locale, url: baseURL! },
        ])
      await page.route("**/api/voice/session", (route) =>
        route.fulfill({
          status: scenario.status,
          json: {
            message:
              "Raw English backend detail that should stay off the alert",
          },
        })
      )
      await page.goto("/voice")
      await page
        .getByRole("button", {
          name: scenario.messages.voice.textInstead,
          exact: true,
        })
        .click()
      await expect(page.getByRole("alert")).toContainText(scenario.expected)
      await expect(
        page.getByText(
          "Raw English backend detail that should stay off the alert"
        )
      ).toHaveCount(0)
      await expectFits(page)
    })
  })
}

for (const [locale, messages] of Object.entries({ en, nl, sw })) {
  for (const textOnly of [true, false]) {
    test(`${locale} initializes native ${textOnly ? "text" : "voice"} in the selected language`, async ({
      page,
      api: _api,
      baseURL,
    }) => {
      void _api
      await page.emulateMedia({ colorScheme: textOnly ? "dark" : "light" })
      await page
        .context()
        .addCookies([{ name: "NEXT_LOCALE", value: locale, url: baseURL! }])
      let language: unknown
      await page.route("**/api/voice/session", (route) => {
        const body = route.request().postDataJSON()
        if (route.request().method() === "DELETE")
          return route.fulfill({ json: { status: "ended" } })
        if (body.command === "heartbeat")
          return route.fulfill({ json: { status: "active" } })
        expect(body).toMatchObject({ locale, textOnly })
        return route.fulfill({
          json: {
            sessionId: "locale-session",
            conversationId: "locale-conversation",
            signedUrl:
              "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
          },
        })
      })
      await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
        socket.onMessage((raw) => {
          const event = JSON.parse(String(raw))
          if (event.type === "conversation_initiation_client_data") {
            language = event.conversation_config_override.agent.language
            socket.send(
              JSON.stringify({
                type: "conversation_initiation_metadata",
                conversation_initiation_metadata_event: {
                  conversation_id: "locale-conversation",
                  agent_output_audio_format: "pcm_16000",
                  user_input_audio_format: "pcm_16000",
                },
              })
            )
          }
        })
      })
      await page.goto("/voice")
      await page
        .getByRole("button", {
          name: textOnly ? messages.voice.textInstead : messages.voice.start,
          exact: true,
        })
        .click()
      await expect.poll(() => language).toBe(locale)
      await expectFits(page)
      await page
        .getByRole("button", { name: messages.voice.end, exact: true })
        .last()
        .click()
    })
  }
}

test("reload recovers authenticated tasks without transcripts, files, or a provider reconnect", async ({
  page,
  api: _api,
}) => {
  void _api
  let starts = 0
  let connections = 0
  let commits = 0
  let restores = 0
  const actionId = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
  const review = () => ({
    ticket: "fresh-secret-review-ticket",
    summary: "Create a recoverable order",
    confirmationCode: "123456",
    confirmationPhrase: "Confirm action 123456",
    target: "Current workspace",
    details: { reference: "Saved server proposal" },
    expiresAt: new Date(Date.now() + 120000).toISOString(),
  })
  await page.route("**/api/voice/session", (route) => {
    if (route.request().method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })
    if (route.request().postDataJSON().command === "heartbeat")
      return route.fulfill({ json: { status: "active" } })
    starts++
    return route.fulfill({
      json: {
        sessionId: "recovery-session",
        conversationId: "recovery-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.route("**/api/voice/tools", (route) => {
    const body = route.request().postDataJSON()
    expect(body).toMatchObject({
      workspaceId: "w1",
      sessionId: "recovery-session",
    })
    expect(route.request().headers().authorization).toBe("Bearer e2e-token")
    if (body.command === "status")
      return route.fulfill({
        json: {
          status: "completed",
          data: [
            {
              status: commits ? "completed" : "awaiting_review",
              message: commits
                ? "Recovered order created."
                : "Awaiting fresh review.",
              data: {
                actionId,
                summary: "Create a recoverable order",
                recoverable: !commits,
              },
            },
            {
              status: "unknown",
              message: "Check the previous invoice before repeating.",
              data: {
                actionId: "other-action",
                summary: "Previous invoice",
                recoverable: false,
              },
            },
            {
              status: "partial",
              message:
                "Verified prefix retained. Review new intent before any remaining work.",
              data: {
                kind: "batch",
                batchId: "recovered-batch",
                summary: "Recovered ordered batch",
                state: "paused",
                recoverable: false,
                steps: [
                  {
                    status: "completed",
                    data: {
                      actionId: "finished-step",
                      summary: "Verified first step",
                    },
                  },
                  {
                    status: "not_started",
                    data: {
                      actionId: "unrun-step",
                      summary: "Unrun second step",
                    },
                  },
                ],
              },
            },
          ],
        },
      })
    if (body.command === "restore") {
      restores++
      expect(body.actionId).toBe(actionId)
    }
    return route.fulfill({
      json: {
        status: "requires_confirmation",
        review: review(),
        data: { actionId },
      },
    })
  })
  await page.route("**/api/voice/confirm", (route) => {
    expect(route.request().postDataJSON()).toMatchObject({
      sessionId: "recovery-session",
      workspaceId: "w1",
      ticket: "fresh-secret-review-ticket",
      decision: "approve",
      method: "click",
    })
    commits++
    return route.fulfill({
      json: { status: "completed", message: "Recovered order created." },
    })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    connections++
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      if (event.type === "conversation_initiation_client_data")
        socket.send(
          JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: "recovery-conversation",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        )
      if (event.type === "user_message")
        socket.send(
          JSON.stringify({
            type: "client_tool_call",
            client_tool_call: {
              tool_name: "execute_genesy_tool",
              tool_call_id: "recovery-proposal",
              parameters: {
                operation_id: "orders_create",
                arguments_json:
                  '{"body":{"reference":"Saved server proposal","lines":[{"product_id":"0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaaa","quantity":"1"}]}}',
              },
            },
          })
        )
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: "Use text instead", exact: true })
    .click()
  await page.getByLabel("Choose reference files").setInputFiles({
    name: "private-reference.pdf",
    mimeType: "application/pdf",
    buffer: referencePdf(),
  })
  // Keep the selected reference local: submit the proposal before selecting an upload.
  await page
    .getByRole("button", { name: "Remove reference 1: private-reference.pdf" })
    .click()
  await page
    .getByLabel("Message the assistant")
    .fill("Private conversation text")
  await page.getByRole("button", { name: "Send", exact: true }).click()
  await expect(
    page.getByRole("dialog", { name: "Review action" })
  ).toBeVisible()
  expect(commits).toBe(0)
  expect(
    await page.evaluate(() => sessionStorage.getItem("genesy.voice.recovery"))
  ).toBe(JSON.stringify({ sessionId: "recovery-session", workspaceId: "w1" }))
  await page.reload()
  await expect(
    page.getByRole("heading", { name: en.voice.recoveryTitle })
  ).toBeVisible()
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toHaveCount(0)
  await expect(page.getByRole("log")).not.toContainText(
    "Private conversation text"
  )
  await expect(page.getByText("private-reference.pdf")).toHaveCount(0)
  await expect(
    page.getByText("Check the previous invoice before repeating.")
  ).toBeVisible()
  const recoveredBatch = page
    .getByRole("listitem")
    .filter({ hasText: "Recovered ordered batch" })
  await expect(recoveredBatch).toContainText("Verified first step")
  await expect(recoveredBatch).toContainText("Unrun second step")
  await expect(recoveredBatch).toContainText("Not run")
  await expect(
    recoveredBatch.getByRole("button", { name: en.voice.recoveryReview })
  ).toHaveCount(0)
  expect(starts).toBe(1)
  expect(connections).toBe(1)
  await page
    .getByRole("button", { name: en.voice.recoveryReview, exact: true })
    .click()
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toContainText(en.voice.recoveredReviewHint)
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toContainText("Saved server proposal")
  expect(restores).toBe(1)
  expect(commits).toBe(0)
  await page
    .getByRole("button", { name: "Approve action", exact: true })
    .click()
  await expect.poll(() => commits).toBe(1)
  await expect(page.getByRole("log")).toContainText(
    en.voice.taskStates.completed
  )
  expect(connections).toBe(1)
  await expectFits(page)
  await page
    .getByRole("button", { name: en.voice.recoveryClose, exact: true })
    .click()
  await expect
    .poll(() =>
      page.evaluate(() => sessionStorage.getItem("genesy.voice.recovery"))
    )
    .toBeNull()
})

test("microphone denial keeps the text connection and keyboard controls usable", async ({
  page,
  api: _api,
}) => {
  void _api
  let starts = 0
  let closed = 0
  let connections = 0
  await page.emulateMedia({ reducedMotion: "reduce", colorScheme: "dark" })
  await page.addInitScript(() => {
    Object.defineProperty(window, "voiceMicrophoneRequests", {
      value: 0,
      writable: true,
    })
    navigator.mediaDevices.getUserMedia = async () => {
      Reflect.set(
        window,
        "voiceMicrophoneRequests",
        Number(Reflect.get(window, "voiceMicrophoneRequests")) + 1
      )
      throw new DOMException("Permission denied", "NotAllowedError")
    }
  })
  await page.route("**/api/voice/session", (route) => {
    if (route.request().method() === "DELETE") {
      closed++
      return route.fulfill({ json: { status: "ended" } })
    }
    if (route.request().postDataJSON()?.command === "heartbeat")
      return route.fulfill({ json: { status: "active" } })
    starts++
    return route.fulfill({
      json: {
        sessionId: "denied-session",
        conversationId: "denied-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    connections++
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      if (event.type === "conversation_initiation_client_data")
        socket.send(
          JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: "denied-conversation",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        )
      if (event.type === "user_message")
        socket.send(
          JSON.stringify({
            type: "agent_response",
            agent_response_event: {
              agent_response: "Text still works.",
              event_id: 2,
            },
          })
        )
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: "Use text instead", exact: true })
    .focus()
  await page.keyboard.press("Enter")
  await expect(page.getByLabel("Message the assistant")).toBeVisible()
  expect(
    await page.evaluate(() => Reflect.get(window, "voiceMicrophoneRequests"))
  ).toBe(0)
  await page.getByRole("button", { name: "Enable voice", exact: true }).focus()
  await page.keyboard.press("Enter")
  await expect(
    page.getByRole("alert").filter({ hasText: en.voice.microphoneDenied })
  ).toBeVisible()
  expect(
    await page.evaluate(() => Reflect.get(window, "voiceMicrophoneRequests"))
  ).toBe(1)
  expect(starts).toBe(1)
  expect(connections).toBe(1)
  expect(closed).toBe(0)
  const viewport = page.viewportSize()!
  await page.setViewportSize({ width: viewport.width, height: 420 })
  await page.getByLabel("Message the assistant").fill("Continue in text")
  await expect(page.getByLabel("Message the assistant")).toHaveAttribute(
    "aria-describedby",
    "voice-file-hint"
  )
  await page.getByRole("button", { name: "Send", exact: true }).focus()
  await page.keyboard.press("Space")
  await expect(page.getByRole("log")).toContainText("Text still works.")
  await expect(page.getByRole("log")).toHaveAttribute("aria-live", "polite")
  await expect(page.locator("[data-motion]")).toHaveCount(0)
  await expectFits(page)
  const endControl = await page
    .getByRole("button", { name: "End conversation", exact: true })
    .boundingBox()
  expect(endControl!.y).toBeGreaterThanOrEqual(0)
  expect(endControl!.y + endControl!.height).toBeLessThanOrEqual(80)
  await page
    .getByRole("button", { name: "End conversation", exact: true })
    .click()
})

test("duplicate reference names retain ordinals and local previews restore keyboard focus", async ({
  page,
  api: _api,
}) => {
  void _api
  let uploads = 0
  await page.route("**/api/voice/attachments", (route) => {
    uploads++
    return route.fulfill({ json: { status: "failed" } })
  })
  await page.route("**/api/voice/session", (route) =>
    route.fulfill({
      json:
        route.request().method() === "DELETE"
          ? { status: "ended" }
          : {
              sessionId: "preview-session",
              conversationId: "preview-conversation",
              signedUrl:
                "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
            },
    })
  )
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) =>
    socket.onMessage((raw) => {
      if (
        JSON.parse(String(raw)).type === "conversation_initiation_client_data"
      )
        socket.send(
          JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: "preview-conversation",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        )
    })
  )
  await page.goto("/voice")
  await page
    .getByRole("button", { name: "Use text instead", exact: true })
    .click()
  const reference = {
    name: "duplicate.pdf",
    mimeType: "application/pdf",
    buffer: referencePdf(),
  }
  await page
    .getByLabel("Choose reference files")
    .setInputFiles([reference, reference])
  await page
    .getByRole("button", { name: /Remove.*duplicate\.pdf/ })
    .first()
    .click()
  const tray = page.getByRole("list", { name: "Session references" })
  await expect(tray).toContainText("2. duplicate.pdf")
  await expect(tray).not.toContainText("1. duplicate.pdf")
  await expect(tray).toContainText(`${reference.buffer.length} B`)
  await page
    .getByRole("button", { name: "Preview reference 2", exact: true })
    .focus()
  await page.keyboard.press("Enter")
  const preview = page.getByRole("dialog", {
    name: "duplicate.pdf",
    exact: true,
  })
  await expect(preview).toBeVisible()
  await expect(
    preview.getByRole("link", { name: "Open local PDF", exact: true })
  ).toHaveAttribute("href", /^blob:/)
  await expect(
    preview.getByRole("link", { name: "Open local PDF", exact: true })
  ).toHaveAttribute("rel", /noopener/)
  await expectFits(page)
  await page.keyboard.press("Escape")
  await expect(preview).toHaveCount(0)
  await expect(
    page.getByRole("button", { name: "Preview reference 2", exact: true })
  ).toBeFocused()
  expect(uploads).toBe(0)
  await page
    .getByRole("button", { name: "End conversation", exact: true })
    .click()
})

test.describe("reference upload retry", () => {
  test.use({
    allow: { responses: [{ url: "/api/voice/attachments", status: 415 }] },
  })
  test("retry sends only the failed reference and preserves the other selection", async ({
    page,
    api: _api,
  }) => {
    void _api
    const uploads: string[] = []
    const shared: string[][] = []
    await page.route("**/api/voice/session", (route) =>
      route.fulfill({
        json:
          route.request().method() === "DELETE"
            ? { status: "ended" }
            : {
                sessionId: "retry-session",
                conversationId: "retry-conversation",
                signedUrl:
                  "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
              },
      })
    )
    await page.route("**/api/voice/attachments", (route) => {
      uploads.push(
        decodeURIComponent(route.request().headers()["x-file-name"]!)
      )
      if (uploads.length === 1)
        return route.fulfill({
          status: 415,
          json: { message: "Unreadable reference." },
        })
      return route.fulfill({
        json: {
          id: `original-${uploads.length}`,
          fileId: `provider-${uploads.length}`,
          status: "uploaded",
        },
      })
    })
    await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) =>
      socket.onMessage((raw) => {
        const event = JSON.parse(String(raw))
        if (event.type === "conversation_initiation_client_data")
          socket.send(
            JSON.stringify({
              type: "conversation_initiation_metadata",
              conversation_initiation_metadata_event: {
                conversation_id: "retry-conversation",
                agent_output_audio_format: "pcm_16000",
                user_input_audio_format: "pcm_16000",
              },
            })
          )
        if (event.type === "multimodal_message")
          shared.push(
            event.files.map((file: { file_id: string }) => file.file_id)
          )
      })
    )
    await page.goto("/voice")
    await page
      .getByRole("button", { name: "Use text instead", exact: true })
      .click()
    await page.getByLabel("Choose reference files").setInputFiles(
      ["first.pdf", "second.pdf"].map((name) => ({
        name,
        mimeType: "application/pdf",
        buffer: referencePdf(),
      }))
    )
    await page.getByRole("button", { name: "Send", exact: true }).click()
    await expect(
      page.getByRole("button", { name: "Retry reference 1", exact: true })
    ).toBeVisible()
    expect(uploads).toEqual(["first.pdf"])
    expect(shared).toHaveLength(0)
    await page
      .getByRole("button", { name: "Retry reference 1", exact: true })
      .click()
    await expect.poll(() => shared).toEqual([["provider-2"]])
    expect(uploads).toEqual(["first.pdf", "first.pdf"])
    const tray = page.getByRole("list", { name: "Session references" })
    await expect(
      tray.getByRole("listitem").filter({ hasText: "second.pdf" })
    ).toContainText("Selected · not sent")
    await page.getByRole("button", { name: "Send", exact: true }).click()
    await expect.poll(() => shared).toEqual([["provider-2"], ["provider-3"]])
    expect(uploads).toEqual(["first.pdf", "first.pdf", "second.pdf"])
    await expectFits(page)
    await page
      .getByRole("button", { name: "End conversation", exact: true })
      .click()
  })
})

test("parallel native preparations keep one exact review and reject the second proposal", async ({
  page,
  api: _api,
}) => {
  void _api
  let preparations = 0
  let commits = 0
  let rejected: unknown
  let previousStatus: unknown
  let release = () => {}
  const firstPreparation = new Promise<void>((resolve) => {
    release = resolve
  })
  await page.route("**/api/voice/session", (route) => {
    if (route.request().method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })
    if (route.request().postDataJSON()?.command === "heartbeat")
      return route.fulfill({ json: { status: "active" } })
    return route.fulfill({
      json: {
        sessionId: "parallel-session",
        conversationId: "parallel-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.route("**/api/voice/tools", async (route) => {
    const body = route.request().postDataJSON()
    if (body.command === "status")
      return route.fulfill({
        json: body.actionId
          ? {
              status: "completed",
              message: "Previous action remains completed.",
            }
          : { status: "completed", data: [] },
      })
    preparations++
    const first = preparations === 1
    if (first) await firstPreparation
    return route.fulfill({
      json: {
        status: "requires_confirmation",
        data: { actionId: first ? "first-action" : "second-action" },
        review: {
          ticket: first ? "first-ticket" : "second-ticket",
          summary: first
            ? "First concurrent proposal"
            : "Second concurrent proposal",
          confirmationCode: "123456",
          confirmationPhrase: "Confirm action 123456",
          target: "Current workspace",
          details: { reference: body.arguments.body.reference },
          expiresAt: new Date(Date.now() + 120000).toISOString(),
        },
      },
    })
  })
  await page.route("**/api/voice/confirm", (route) => {
    expect(route.request().postDataJSON()).toMatchObject({
      ticket: "first-ticket",
      decision: "approve",
      method: "click",
    })
    commits++
    return route.fulfill({
      json: {
        status: "completed",
        message: "Only the first reviewed proposal was created.",
      },
    })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      if (event.type === "conversation_initiation_client_data")
        socket.send(
          JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: "parallel-conversation",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        )
      if (event.type === "user_message") {
        for (const position of ["first", "second"])
          socket.send(
            JSON.stringify({
              type: "client_tool_call",
              client_tool_call: {
                tool_name: "execute_genesy_tool",
                tool_call_id: position,
                parameters: {
                  operation_id: "orders_create",
                  arguments_json: JSON.stringify({
                    body: {
                      reference: position,
                      lines: [
                        {
                          product_id: "0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaaa",
                          quantity: "1",
                        },
                      ],
                    },
                  }),
                },
              },
            })
          )
        socket.send(
          JSON.stringify({
            type: "client_tool_call",
            client_tool_call: {
              tool_name: "execute_genesy_tool",
              tool_call_id: "previous-status",
              parameters: {
                operation_id: "assistant_task_status",
                arguments_json: JSON.stringify({
                  actionId: "0193f6b0-1c2d-7e3f-8a9b-ffffffffffff",
                  wait: true,
                }),
              },
            },
          })
        )
      }
      if (
        event.type === "client_tool_result" &&
        event.tool_call_id === "previous-status"
      )
        previousStatus = JSON.parse(event.result)
      if (
        event.type === "client_tool_result" &&
        event.tool_call_id === "second"
      )
        rejected = JSON.parse(event.result)
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: "Use text instead", exact: true })
    .click()
  await page.getByLabel("Message the assistant").fill("Prepare both requests")
  await page.getByRole("button", { name: "Send", exact: true }).click()
  await expect.poll(() => rejected).toMatchObject({ status: "failed" })
  await expect
    .poll(() => previousStatus)
    .toMatchObject({
      status: "completed",
      message: "Previous action remains completed.",
    })
  expect(preparations).toBe(1)
  expect(commits).toBe(0)
  release()
  await expect(
    page.getByRole("dialog", { name: "Review action" })
  ).toContainText("First concurrent proposal")
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).not.toContainText("Second concurrent proposal")
  await expectFits(page)
  await page
    .getByRole("button", { name: "Approve action", exact: true })
    .click()
  await expect.poll(() => commits).toBe(1)
  await expect(page.getByRole("log")).toContainText(
    en.voice.taskStates.completed
  )
  expect(preparations).toBe(1)
  await page
    .getByRole("button", { name: "End conversation", exact: true })
    .click()
})

for (const finalStatus of ["completed", "cancelled", "unknown"] as const) {
  const approved = finalStatus !== "cancelled"
  test(`voice starts on demand and reports ${finalStatus} through native tool results`, async ({
    page,
    api: _api,
  }) => {
    void _api
    let commits = 0
    let userMessages = 0
    let nativeResult: unknown
    const actionId = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
    const finalMessage =
      finalStatus === "completed"
        ? "Your draft order is created."
        : finalStatus === "cancelled"
          ? "The action was cancelled."
          : "Completion is uncertain. Check the order before repeating."
    let releaseCommit = () => {}
    const commitGate = new Promise<void>((resolve) => {
      releaseCommit = resolve
    })
    if (approved) await page.clock.install()
    await page.route("**/api/voice/session", (route) => {
      if (
        route.request().method() === "POST" &&
        route.request().postDataJSON()?.command === "heartbeat"
      )
        return route.fulfill({ json: { status: "active" } })
      return route.fulfill({
        json: {
          sessionId: "session-one",
          conversationId: "conv-e2e",
          signedUrl:
            "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
        },
      })
    })
    await page.route("**/api/voice/tools", async (route) => {
      const body = route.request().postDataJSON()
      expect(body.workspaceId).toBe("w1")
      expect(body.sessionId).toBe("session-one")
      expect(route.request().headers().authorization).toBe("Bearer e2e-token")
      if (body.command === "status")
        return route.fulfill({
          json: body.actionId
            ? { status: "awaiting_review", data: { actionId } }
            : { status: "completed", data: [] },
        })

      return route.fulfill({
        json: {
          status: "requires_confirmation",
          data: { actionId },
          review: {
            ticket: "review-ticket",
            summary: "Create a draft order",
            confirmationCode: "123456",
            confirmationPhrase: "Confirm action 123456",
            target: "Current workspace",
            details: {
              workspace: "0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaaa",
              target: {
                order: "0193f6b0-1c2d-7e3f-8a9b-bbbbbbbbbbbb",
                warehouse: "0193f6b0-1c2d-7e3f-8a9b-cccccccccccc",
              },
              record: { name: "Ilundo Honey Co. · reviewed warehouse" },
              reference: "Voice order",
              items: Array.from({ length: 31 }, (_, i) => ({
                label: `Reviewed line ${i + 1}`,
              })),
              scope: {
                warehouse: { bin: { lot: { code: "Nested reviewed lot" } } },
              },
            },
            expiresAt: new Date(Date.now() + 120000).toISOString(),
          },
        },
      })
    })
    await page.route("**/api/voice/confirm", async (route) => {
      const body = route.request().postDataJSON()
      expect(body.sessionId).toBe("session-one")
      expect(body.method).toBe("click")
      if (body.decision === "approve") {
        commits++
        await commitGate
      }
      return route.fulfill({
        json: {
          status: finalStatus,
          message: finalMessage,
        },
      })
    })
    await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
      socket.onMessage((raw) => {
        const event = JSON.parse(String(raw))
        if (event.type === "conversation_initiation_client_data") {
          expect(event.conversation_config_override.agent.language).toBe("en")
          socket.send(
            JSON.stringify({
              type: "conversation_initiation_metadata",
              conversation_initiation_metadata_event: {
                conversation_id: "conv-e2e",
                agent_output_audio_format: "pcm_16000",
                user_input_audio_format: "pcm_16000",
              },
            })
          )
          socket.send(
            JSON.stringify({
              type: "agent_response",
              agent_response_event: {
                agent_response: "What can I help you with?",
              },
            })
          )
        }
        if (event.type === "user_message") {
          userMessages++
          socket.send(
            JSON.stringify({
              type: "client_tool_call",
              client_tool_call: {
                tool_name: "execute_genesy_tool",
                tool_call_id: "call-one",
                parameters: {
                  operation_id: "orders_create",
                  arguments_json:
                    '{"body":{"reference":"Voice order","lines":[{"product_id":"0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaaa","quantity":"1"}]}}',
                },
              },
            })
          )
        }
        if (event.type === "client_tool_result") {
          if (event.tool_call_id === "watch-one") {
            nativeResult = JSON.parse(event.result)
            expect(nativeResult).toMatchObject({
              status: finalStatus,
              message: finalMessage,
            })
            socket.send(
              JSON.stringify({
                type: "agent_response",
                agent_response_event: {
                  agent_response: `Recorded result: ${finalMessage}`,
                  event_id: 10,
                },
              })
            )
            return
          }
          expect(event.result).not.toContain("review-ticket")
          expect(event.result).not.toContain("Voice order")
          socket.send(
            JSON.stringify({
              type: "agent_response",
              agent_response_event: {
                agent_response:
                  JSON.parse(event.result).status === "requires_confirmation"
                    ? "Review the proposed change."
                    : "Result received.",
              },
            })
          )
          socket.send(
            JSON.stringify({
              type: "client_tool_call",
              client_tool_call: {
                tool_name: "execute_genesy_tool",
                tool_call_id: "watch-one",
                parameters: {
                  operation_id: "assistant_task_status",
                  arguments_json: JSON.stringify({ actionId, wait: true }),
                },
              },
            })
          )
        }
      })
    })
    await page.goto("/voice")
    await expect(
      page.getByRole("dialog", { name: en.voice.title, exact: true })
    ).toBeVisible()
    await expect(
      page.getByRole("button", { name: "Start voice conversation" })
    ).toBeVisible()
    await expectFits(page)
    await page.getByRole("button", { name: "Use text instead" }).click()
    await expect(page.getByRole("log")).toContainText(
      "What can I help you with?"
    )
    await page.getByLabel("Message the assistant").fill("Create a draft order")
    await page.getByRole("button", { name: "Send", exact: true }).click()
    await expect(
      page.getByRole("dialog", { name: "Review action" })
    ).toBeVisible()
    expect(commits).toBe(0)
    await expect(
      page.getByRole("dialog", { name: "Review action" })
    ).toContainText("Voice order")
    await expect(
      page.getByRole("dialog", { name: "Review action" })
    ).toContainText("Reviewed line 31")
    await expect(
      page.getByRole("dialog", { name: "Review action" })
    ).toContainText("Nested reviewed lot")
    const approval = page.getByRole("dialog", {
      name: en.voice.review,
      exact: true,
    })
    for (const id of [
      "0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaaa",
      "0193f6b0-1c2d-7e3f-8a9b-bbbbbbbbbbbb",
      "0193f6b0-1c2d-7e3f-8a9b-cccccccccccc",
    ])
      await expect(approval).not.toContainText(id)
    await expect(
      approval.getByText("Ilundo Honey Co. · reviewed warehouse", {
        exact: true,
      })
    ).toBeVisible()
    await expect(approval).toContainText(en.voice.currentWorkspace)
    await expect(approval).toContainText(
      en.voice.recordReference.replace("{number}", "2")
    )
    await expect(approval).toContainText(
      en.voice.recordReference.replace("{number}", "3")
    )
    await expectFits(page)
    const readsBeforeApproval = _api.callsTo("GET", "/me/workspaces").length
    if (approved) await page.clock.fastForward(59000)
    await page
      .getByRole("button", {
        name: approved ? "Approve action" : "Cancel",
        exact: true,
      })
      .click()
    if (approved) {
      await expect.poll(() => commits).toBe(1)
      await page.clock.fastForward(2000)
      await expect(
        page.getByRole("dialog", { name: "Review action" })
      ).toBeVisible()
      releaseCommit()
    }
    await expect(page.getByRole("log")).toContainText(finalMessage)
    await expect.poll(() => nativeResult).toMatchObject({ status: finalStatus })
    await expect(page.getByRole("log")).toContainText(
      `Recorded result: ${finalMessage}`
    )
    expect(userMessages).toBe(1)
    expect(commits).toBe(approved ? 1 : 0)
    if (approved)
      await expect
        .poll(() => _api.callsTo("GET", "/me/workspaces").length)
        .toBeGreaterThan(readsBeforeApproval)
    await expectFits(page)
    await page.getByRole("button", { name: "End conversation" }).click()
    await expect(
      page.getByRole("button", { name: "Start voice conversation" })
    ).toBeVisible()
  })
}

for (const finalStatus of ["completed", "partial", "unknown"] as const) {
  test(`one private ordered batch review reports ${finalStatus} without replay`, async ({
    page,
    api: _api,
  }) => {
    void _api
    const batchId = "0193f6b0-1c2d-7e3f-8a9b-dddddddddddd"
    const actionIds = ["aaaaaaaaaaaa", "bbbbbbbbbbbb", "cccccccccccc"].map(
      (tail) => `0193f6b0-1c2d-7e3f-8a9b-${tail}`
    )
    let approvals = 0
    let prepares = 0
    let userMessages = 0
    let nativeResult: unknown
    const finalMessage =
      finalStatus === "completed"
        ? "All three reviewed steps completed."
        : finalStatus === "partial"
          ? "The batch stopped after a failed step. Remaining steps did not run."
          : "A step could not be verified. Remaining steps did not run."
    const outcome = () => ({
      status: approvals ? finalStatus : "awaiting_review",
      message: approvals ? finalMessage : "Review the ordered batch.",
      data: {
        batchId,
        state: approvals
          ? finalStatus === "completed"
            ? "completed"
            : "paused"
          : "proposed",
        nextIndex: approvals ? (finalStatus === "completed" ? 3 : 1) : 0,
        recoverable: false,
        steps: actionIds.map((actionId, index) => ({
          status: approvals
            ? finalStatus === "completed" || index === 0
              ? "completed"
              : index === 1
                ? finalStatus === "unknown"
                  ? "unknown"
                  : "failed"
                : "not_started"
            : "not_started",
          data: { actionId, summary: `Reviewed step ${index + 1}` },
        })),
      },
    })
    await page.route("**/api/voice/session", (route) =>
      route.fulfill({
        json:
          route.request().method() === "DELETE"
            ? { status: "ended" }
            : route.request().postDataJSON()?.command === "heartbeat"
              ? { status: "active" }
              : {
                  sessionId: "batch-session",
                  conversationId: "batch-conversation",
                  signedUrl:
                    "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
                },
      })
    )
    await page.route("**/api/voice/tools", (route) =>
      route.fulfill({
        json:
          route.request().postDataJSON()?.command === "status"
            ? { status: "completed", data: [] }
            : {
                status: "failed",
                message: "Fixed batches use their dedicated preparation route.",
              },
      })
    )
    await page.route("**/api/voice/batch", (route) => {
      const body = route.request().postDataJSON()
      expect(body).toMatchObject({
        workspaceId: "w1",
        sessionId: "batch-session",
      })
      if (body.command === "status") return route.fulfill({ json: outcome() })
      expect(body.command).toBe("prepare")
      expect(body.calls).toHaveLength(3)
      prepares++
      return route.fulfill({
        json: {
          status: "requires_confirmation",
          data: { batchId, actionIds },
          review: {
            kind: "batch",
            batchId,
            ticket: "private-batch-ticket",
            summary: "Three ordered changes",
            target: "Current workspace",
            details: { workspace: "w1" },
            steps: actionIds.map((actionId, index) => ({
              actionId,
              summary: `Reviewed step ${index + 1}`,
              target: `Order ${index + 1}`,
              details: {
                before: "2.00",
                after: "3.00",
                quantity: "1.00 kg",
                total: "€1.25",
                reference: `Batch line ${index + 1}`,
              },
            })),
            confirmationCode: "246810",
            confirmationPhrase: "Confirm batch 246810",
            expiresAt: new Date(Date.now() + 120000).toISOString(),
          },
        },
      })
    })
    await page.route("**/api/voice/confirm", (route) => {
      expect(route.request().postDataJSON()).toMatchObject({
        ticket: "private-batch-ticket",
        decision: "approve",
        method: "click",
        sessionId: "batch-session",
      })
      approvals++
      return route.fulfill({ json: outcome() })
    })
    await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) =>
      socket.onMessage((raw) => {
        const event = JSON.parse(String(raw))
        if (["client_tool_result", "contextual_update"].includes(event.type))
          for (const privateValue of [
            "246810",
            "private-batch-ticket",
            "confirmationCode",
            "confirmationPhrase",
          ])
            expect(String(raw)).not.toContain(privateValue)
        if (event.type === "conversation_initiation_client_data")
          socket.send(
            JSON.stringify({
              type: "conversation_initiation_metadata",
              conversation_initiation_metadata_event: {
                conversation_id: "batch-conversation",
                agent_output_audio_format: "pcm_16000",
                user_input_audio_format: "pcm_16000",
              },
            })
          )
        if (event.type === "user_message") {
          userMessages++
          socket.send(
            JSON.stringify({
              type: "client_tool_call",
              client_tool_call: {
                tool_name: "execute_genesy_tool",
                tool_call_id: "batch-prepare",
                parameters: {
                  operation_id: "assistant_batch_prepare",
                  arguments_json: JSON.stringify({
                    calls: actionIds.map((id, index) => ({
                      operationId: "orders_update",
                      arguments: {
                        path: { order: id },
                        body: { reference: `Batch line ${index + 1}` },
                      },
                    })),
                  }),
                },
              },
            })
          )
        }
        if (
          event.type === "client_tool_result" &&
          event.tool_call_id === "batch-prepare"
        ) {
          const result = JSON.parse(event.result)
          expect(result).toMatchObject({
            status: "requires_confirmation",
            batchId,
          })
          socket.send(
            JSON.stringify({
              type: "client_tool_call",
              client_tool_call: {
                tool_name: "execute_genesy_tool",
                tool_call_id: "batch-watch",
                parameters: {
                  operation_id: "assistant_batch_status",
                  arguments_json: JSON.stringify({ batchId, wait: true }),
                },
              },
            })
          )
        }
        if (
          event.type === "client_tool_result" &&
          event.tool_call_id === "batch-watch"
        ) {
          nativeResult = JSON.parse(event.result)
          socket.send(
            JSON.stringify({
              type: "agent_response",
              agent_response_event: {
                agent_response: `Verified batch result: ${finalMessage}`,
                event_id: 20,
              },
            })
          )
        }
      })
    )
    await page.goto("/voice")
    await page
      .getByRole("button", { name: "Use text instead", exact: true })
      .click()
    await page.getByLabel("Message the assistant").fill("Prepare a fixed batch")
    await page.getByRole("button", { name: "Send", exact: true }).click()
    const review = page.getByRole("dialog", { name: "Review action" })
    await expect(review).toBeVisible()
    // One row per step, numbered in the reviewed order, none hidden.
    const rows = review.locator('.voice-review-list [role="option"]')
    await expect(rows).toHaveCount(3)
    for (let i = 0; i < 3; i++) {
      await expect(rows.nth(i)).toBeVisible()
      await expect(rows.nth(i)).toContainText(`${i + 1}.`)
      await expect(rows.nth(i)).toContainText(`Batch line ${i + 1}`)
    }
    for (const value of [
      "Reviewed step 1",
      "Reviewed step 2",
      "Reviewed step 3",
      "2.00",
      "3.00",
      "1.00 kg",
      "€1.25",
      "Confirm batch 246810",
    ])
      await expect(review).toContainText(value)
    expect(approvals).toBe(0)
    await expectFits(page)
    await page
      .getByRole("button", { name: "Approve action", exact: true })
      .click()
    await expect.poll(() => nativeResult).toMatchObject({ status: finalStatus })
    await expect(page.getByRole("log")).toContainText(
      `Verified batch result: ${finalMessage}`
    )
    await expect(
      page
        .getByRole("region", { name: en.voice.currentTask, exact: true })
        .locator("[data-task-outcome]")
        .first()
    ).toHaveText(en.voice.taskStates[finalStatus])
    await expect(
      page.getByRole("region", { name: "Batch progress" })
    ).toContainText("Reviewed step 3")
    if (finalStatus !== "completed")
      await expect(
        page.getByRole("region", { name: "Batch progress" })
      ).toContainText("Not run")
    expect(approvals).toBe(1)
    expect(prepares).toBe(1)
    expect(userMessages).toBe(1)
    await expectFits(page)
    await page
      .getByRole("button", { name: "End conversation", exact: true })
      .click()
  })
}

for (const interruption of [
  "changed intent",
  "disconnect",
  "hidden tab",
  "stop remaining",
] as const) {
  test(`an interrupted batch pauses remaining steps on ${interruption}`, async ({
    page,
    api: _api,
  }) => {
    void _api
    let prepares = 0
    let approvals = 0
    let cancellations = 0
    let pauses = 0
    let emit: (event: Record<string, unknown>) => void = () => {}
    let disconnect = () => {}
    let release = () => {}
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const batchId = () => `0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaa${prepares}`
    const stopped = () => ({
      status: "partial",
      message: "First step verified. Remaining steps did not run.",
      data: {
        kind: "batch",
        batchId: batchId(),
        state: "paused",
        nextIndex: 1,
        recoverable: false,
        steps: [
          { status: "completed", data: { summary: "First reviewed step" } },
          {
            status: "not_started",
            data: { summary: "Second reviewed step" },
          },
        ],
      },
    })
    await page.route("**/api/voice/session", (route) =>
      route.fulfill({
        json:
          route.request().method() === "DELETE"
            ? { status: "ended" }
            : route.request().postDataJSON()?.command === "heartbeat"
              ? { status: "active" }
              : {
                  sessionId: "interrupted-session",
                  conversationId: "interrupted-conversation",
                  signedUrl:
                    "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
                },
      })
    )
    await page.route("**/api/voice/batch", (route) => {
      const body = route.request().postDataJSON()
      if (body.command === "pause") {
        pauses++
        release()
        return route.fulfill({ json: stopped() })
      }
      if (body.command === "cancel" && interruption === "stop remaining") {
        pauses++
        return route.fulfill({ json: stopped() })
      }
      if (body.command === "cancel" || body.command === "status")
        return route.fulfill({ json: stopped() })
      expect(body.command).toBe("prepare")
      prepares++
      return route.fulfill({
        json: {
          status: "requires_confirmation",
          data: { batchId: batchId() },
          review: {
            kind: "batch",
            batchId: batchId(),
            ticket: `interrupt-ticket-${prepares}`,
            summary: "Two exact ordered steps",
            target: "Current workspace",
            details: {},
            steps: ["First reviewed step", "Second reviewed step"].map(
              (summary, index) => ({
                actionId: `action-${index}`,
                summary,
                details: { reference: `Exact step ${index + 1}` },
              })
            ),
            confirmationCode: prepares === 1 ? "111111" : "246810",
            confirmationPhrase: `Confirm batch ${prepares === 1 ? "111111" : "246810"}`,
            expiresAt: new Date(Date.now() + 120000).toISOString(),
          },
        },
      })
    })
    await page.route("**/api/voice/confirm", async (route) => {
      const body = route.request().postDataJSON()
      if (body.decision === "cancel") {
        cancellations++
        return route.fulfill({
          json: {
            status: "cancelled",
            data: { batchId: batchId(), state: "cancelled", steps: [] },
          },
        })
      }
      expect(body).toMatchObject({
        ticket: "interrupt-ticket-2",
        method: "speech",
        eventId: "51",
        confirmationCode: "246810",
        utterance: "Confirm batch 246810",
      })
      approvals++
      await gate
      return route.fulfill({ json: stopped() })
    })
    await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
      emit = (event) => socket.send(JSON.stringify(event))
      disconnect = () => socket.close()
      socket.onMessage((raw) => {
        const event = JSON.parse(String(raw))
        if (["client_tool_result", "contextual_update"].includes(event.type)) {
          for (const value of [
            "111111",
            "246810",
            "interrupt-ticket",
            "confirmationCode",
            "confirmationPhrase",
          ])
            expect(String(raw)).not.toContain(value)
        }
        if (event.type === "conversation_initiation_client_data")
          emit({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: "interrupted-conversation",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        if (event.type === "user_message")
          emit({
            type: "client_tool_call",
            client_tool_call: {
              tool_name: "execute_genesy_tool",
              tool_call_id: `batch-${prepares}`,
              parameters: {
                operation_id: "assistant_batch_prepare",
                arguments_json: JSON.stringify({
                  calls: [
                    {
                      operationId: "orders_update",
                      arguments: {
                        path: { order: "first" },
                        body: { reference: "one" },
                      },
                    },
                    {
                      operationId: "orders_update",
                      arguments: {
                        path: { order: "second" },
                        body: { reference: "two" },
                      },
                    },
                  ],
                }),
              },
            },
          })
      })
    })
    await page.goto("/voice")
    await page
      .getByRole("button", { name: "Use text instead", exact: true })
      .click()
    const ask = async () => {
      await page
        .getByLabel("Message the assistant")
        .fill("Prepare both exact steps")
      await page.getByRole("button", { name: "Send", exact: true }).click()
      await expect(
        page.getByRole("dialog", { name: "Review action" })
      ).toBeVisible()
    }
    await ask()
    emit({
      type: "user_transcript",
      user_transcription_event: {
        user_transcript: "Change the plan first",
        event_id: 50,
      },
    })
    await expect.poll(() => cancellations).toBe(1)
    await expect(
      page.getByRole("dialog", { name: en.voice.review, exact: true })
    ).toHaveCount(0)
    expect(approvals).toBe(0)
    await ask()
    emit({
      type: "user_transcript",
      user_transcription_event: {
        user_transcript: "Confirm batch 246810",
        event_id: 51,
      },
    })
    await expect.poll(() => approvals).toBe(1)
    const readsBeforeStop = _api.callsTo("GET", "/me/workspaces").length
    if (interruption === "changed intent")
      emit({
        type: "user_transcript",
        user_transcription_event: {
          user_transcript: "Stop and change the second step",
          event_id: 52,
        },
      })
    else if (interruption === "disconnect") disconnect()
    else if (interruption === "stop remaining")
      await page
        .getByRole("button", { name: "Stop remaining steps", exact: true })
        .click()
    else
      await page.evaluate(() => {
        Object.defineProperty(document, "visibilityState", {
          value: "hidden",
          configurable: true,
        })
        document.dispatchEvent(new Event("visibilitychange"))
      })
    await expect.poll(() => pauses).toBe(1)
    if (interruption === "stop remaining") {
      await expect
        .poll(() => _api.callsTo("GET", "/me/workspaces").length)
        .toBeGreaterThan(readsBeforeStop)
      release()
      await expect(
        page.getByRole("dialog", { name: en.voice.review, exact: true })
      ).toHaveCount(0)
    }
    const progress = page.getByRole("region", { name: "Batch progress" })
    await expect(progress).toContainText("First reviewed step")
    await expect(progress).toContainText("Second reviewed step")
    await expect(progress).toContainText("Not run")
    expect(approvals).toBe(1)
    expect(prepares).toBe(2)
    await expectFits(page)
    if (interruption !== "disconnect")
      await page
        .getByRole("button", { name: "End conversation", exact: true })
        .click()
  })
}

test("a review arriving after the tab hides is cancelled without exposing or dispatching it", async ({
  page,
  api: _api,
}) => {
  void _api
  let prepares = 0
  let cancellations = 0
  let emit: (event: Record<string, unknown>) => void = () => {}
  const results: Record<string, { status: string }> = {}
  let release = () => {}
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  await page.route("**/api/voice/session", (route) =>
    route.fulfill({
      json:
        route.request().method() === "DELETE"
          ? { status: "ended" }
          : {
              sessionId: "late-session",
              conversationId: "late-conversation",
              signedUrl:
                "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
            },
    })
  )
  await page.route("**/api/voice/tools", async (route) => {
    if (route.request().postDataJSON().command === "status")
      return route.fulfill({ json: { status: "completed", data: [] } })
    prepares++
    await gate
    return route.fulfill({
      json: {
        status: "requires_confirmation",
        data: { actionId: "late-action" },
        review: {
          ticket: "private-late-ticket",
          summary: "Late draft",
          confirmationCode: "456789",
          confirmationPhrase: "Confirm action 456789",
          details: { reference: "Unapproved late intent" },
          expiresAt: new Date(Date.now() + 120000).toISOString(),
        },
      },
    })
  })
  await page.route("**/api/voice/confirm", (route) => {
    expect(route.request().postDataJSON()).toMatchObject({
      ticket: "private-late-ticket",
      decision: "cancel",
      sessionId: "late-session",
    })
    cancellations++
    return route.fulfill({ json: { status: "cancelled" } })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    emit = (event) => socket.send(JSON.stringify(event))
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      if (event.type === "conversation_initiation_client_data")
        emit({
          type: "conversation_initiation_metadata",
          conversation_initiation_metadata_event: {
            conversation_id: "late-conversation",
            agent_output_audio_format: "pcm_16000",
            user_input_audio_format: "pcm_16000",
          },
        })
      if (event.type === "user_message")
        emit({
          type: "client_tool_call",
          client_tool_call: {
            tool_name: "execute_genesy_tool",
            tool_call_id: "first",
            parameters: {
              operation_id: "orders_update",
              arguments_json:
                '{"path":{"order":"one"},"body":{"reference":"Late draft"}}',
            },
          },
        })
      if (event.type === "client_tool_result") {
        expect(String(raw)).not.toContain("456789")
        expect(String(raw)).not.toContain("private-late-ticket")
        results[event.tool_call_id] = JSON.parse(event.result)
      }
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: "Use text instead", exact: true })
    .click()
  await page.getByLabel("Message the assistant").fill("Prepare a late draft")
  await page.getByRole("button", { name: "Send", exact: true }).click()
  await expect.poll(() => prepares).toBe(1)
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", {
      value: "hidden",
      configurable: true,
    })
    document.dispatchEvent(new Event("visibilitychange"))
  })
  release()
  await expect.poll(() => cancellations).toBe(1)
  await expect.poll(() => results.first).toMatchObject({ status: "cancelled" })
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toHaveCount(0)
  emit({
    type: "client_tool_call",
    client_tool_call: {
      tool_name: "execute_genesy_tool",
      tool_call_id: "hidden-again",
      parameters: {
        operation_id: "orders_update",
        arguments_json:
          '{"path":{"order":"one"},"body":{"reference":"Late draft"}}',
      },
    },
  })
  await expect
    .poll(() => results["hidden-again"])
    .toMatchObject({ status: "paused" })
  expect(prepares).toBe(1)
  await page
    .getByRole("button", { name: "End conversation", exact: true })
    .click()
})

test("voice opens the microphone and audio worklets under the page security policy", async ({
  page,
  api: _api,
}) => {
  void _api
  let audioStarts = 0
  let audioConnections = 0
  await page.addInitScript(() => {
    Reflect.set(window, "voiceEnergy", 0)
    AnalyserNode.prototype.getByteFrequencyData = (data) => {
      data.fill(Number(Reflect.get(window, "voiceEnergy")))
    }
  })
  await page.route("**/api/voice/session", (route) => {
    if (route.request().method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })
    if (
      route.request().method() === "POST" &&
      route.request().postDataJSON()?.command === "heartbeat"
    )
      return route.fulfill({ json: { status: "active" } })
    audioStarts++
    return route.fulfill({
      json: {
        sessionId: `session-audio-${audioStarts}`,
        conversationId: `conv-audio-${audioStarts}`,
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=audio-test",
      },
    })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    const connection = ++audioConnections
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      if (event.type === "conversation_initiation_client_data") {
        socket.send(
          JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: `conv-audio-${connection}`,
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        )
        socket.send(
          JSON.stringify({
            type: "agent_response",
            agent_response_event: { agent_response: "Voice is connected." },
          })
        )
        socket.send(
          JSON.stringify({
            type: "audio",
            audio_event: {
              audio_base_64: Buffer.alloc(640).toString("base64"),
              event_id: 1,
            },
          })
        )
      }
    })
  })
  await page.goto("/orders")
  await openMenu(page)
  await page.getByRole("link", { name: "Voice assistant", exact: true }).click()
  await page.getByRole("button", { name: "Start voice conversation" }).click()
  await expect(
    page.getByRole("button", { name: "End conversation" })
  ).toBeVisible()
  await expect(page.getByRole("log")).toContainText("Voice is connected.")
  const emblem = page.locator("[data-voice-orb]")
  await emblem.scrollIntoViewIfNeeded()
  await expect(emblem).toHaveAttribute("data-active", "false")
  await page.evaluate(() => Reflect.set(window, "voiceEnergy", 255))
  await expect(emblem).toHaveAttribute("data-active", "true")
  await emblem.evaluate((el) => {
    el.style.transform = "translateY(-2000px)"
  })
  await expect(emblem).toHaveAttribute("data-animate", "false")
  await emblem.evaluate((el) => {
    el.style.transform = ""
  })
  await expect(emblem).toHaveAttribute("data-active", "true")
  await page.evaluate(() => Reflect.set(window, "voiceEnergy", 0))
  await expect(emblem).toHaveAttribute("data-active", "false")
  await page.evaluate(() => Reflect.set(window, "voiceEnergy", 255))
  await expect(emblem).toHaveAttribute("data-active", "true")
  await page.getByRole("button", { name: "Mute", exact: true }).click()
  await expect(
    page.getByRole("button", { name: "Unmute", exact: true })
  ).toBeVisible()
  await expect(emblem).toHaveAttribute("data-animate", "false")
  await page.getByRole("button", { name: "Unmute", exact: true }).click()
  await expect(emblem).toHaveAttribute("data-active", "true")
  await page.emulateMedia({ reducedMotion: "reduce" })
  await expect(emblem).toHaveAttribute("data-animate", "false")
  await page.emulateMedia({ reducedMotion: "no-preference" })
  await expect(emblem).toHaveAttribute("data-animate", "true")
  const picker = page.waitForEvent("filechooser")
  await page.getByRole("button", { name: "Take photo", exact: true }).click()
  await (
    await picker
  ).setFiles({
    name: "camera.png",
    mimeType: "image/png",
    buffer: Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
      "base64"
    ),
  })
  await expect(
    page.getByRole("button", { name: "Resume voice", exact: true })
  ).toBeVisible()
  expect(audioStarts).toBe(1)
  expect(audioConnections).toBe(1)
  await page
    .getByRole("button", { name: "Preview reference 1", exact: true })
    .click()
  await expect(
    page
      .getByRole("dialog", { name: "camera.png", exact: true })
      .getByRole("img", { name: "camera.png", exact: true })
  ).toBeVisible()
  await page.keyboard.press("Escape")
  await expect(
    page.getByRole("dialog", { name: "camera.png", exact: true })
  ).toHaveCount(0)
  await expect(
    page.getByRole("dialog", { name: en.voice.title, exact: true })
  ).toBeVisible()
  await page.getByRole("button", { name: "Resume voice", exact: true }).click()
  await expect(
    page.getByRole("button", { name: "Mute", exact: true })
  ).toBeVisible()
  await expect.poll(() => audioStarts).toBe(2)
  expect(audioConnections).toBe(2)
  await expect(
    page.getByRole("list", { name: "Session references" })
  ).toContainText("camera.png")
  await expect(
    page.getByRole("list", { name: "Session references" })
  ).toContainText("Selected · not sent")
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", {
      value: "hidden",
      configurable: true,
    })
    document.dispatchEvent(new Event("visibilitychange"))
  })
  await expect(page.locator("[data-motion]")).toHaveCount(0)
  await expect(
    page.getByRole("button", { name: "Resume voice", exact: true })
  ).toBeVisible()
  await expectFits(page)
  await page.getByRole("button", { name: "End conversation" }).click()
  await expect(page.locator("[data-motion]")).toHaveCount(0)
})

test("text conversation survives navigation and shares a file without reconnecting", async ({
  page,
  api: _api,
}) => {
  emptyOverview(_api)
  let starts = 0
  let connections = 0
  let uploads = 0
  let multimodalCount = 0
  let multimodal: unknown
  let approvedUploads = 0
  let carriedStatus: unknown
  const contextual: string[] = []
  await page.route("**/api/voice/session", (route) => {
    if (
      route.request().method() === "POST" &&
      route.request().postDataJSON()?.command === "heartbeat"
    )
      return route.fulfill({ json: { status: "active" } })
    if (route.request().method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })
    starts++
    return route.fulfill({
      json: {
        sessionId: starts === 1 ? "session-one" : "session-two",
        conversationId: starts === 1 ? "conv-one" : "conv-two",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.route("**/api/voice/attachments", async (route) => {
    expect(route.request().headers()["x-workspace-id"]).toBe("w1")
    expect(route.request().headers()["x-voice-session"]).toBe(
      starts === 1 ? "session-one" : "session-two"
    )
    expect(route.request().headers()["content-type"]).toBe("application/pdf")
    uploads++
    return route.fulfill({
      json: {
        id: `local-${uploads}`,
        fileId: `provider-${uploads}`,
        status: "uploaded",
      },
    })
  })
  await page.route("**/api/voice/tools", (route) => {
    if (route.request().headers()["content-type"] === "application/json") {
      const body = route.request().postDataJSON()
      if (body.command === "status" && body.actionId) {
        expect(body).toMatchObject({
          sessionId: "session-one",
          actionId: "0193f6b0-1c2d-7e3f-8a9b-dddddddddddd",
        })
        return route.fulfill({
          json: {
            status: "completed",
            data: { actionId: body.actionId },
            message: "Verified original saved file.",
          },
        })
      }
      return route.fulfill({ json: { status: "completed", data: [] } })
    }
    expect(route.request().headers()["content-type"]).toContain(
      "multipart/form-data; boundary="
    )
    const bytes = route.request().postDataBuffer()!.toString("utf8")
    expect(bytes).toContain('name="payload"')
    expect(bytes).toContain('name="local-1"; filename="reference.pdf"')
    expect(bytes).toContain("Customer reference ABC")
    expect(bytes).not.toContain('name="provider-1"')
    return route.fulfill({
      json: {
        status: "requires_confirmation",
        data: { actionId: "0193f6b0-1c2d-7e3f-8a9b-dddddddddddd" },
        review: {
          ticket: "upload-review",
          summary: "Save reference to Files",
          confirmationCode: "654321",
          confirmationPhrase: "Confirm action 654321",
          target: "Files",
          details: { files: 1 },
          attachments: { upload: ["local-1"] },
          expiresAt: new Date(Date.now() + 120000).toISOString(),
        },
      },
    })
  })
  await page.route("**/api/voice/confirm", (route) => {
    expect(route.request().headers()["content-type"]).toContain(
      "multipart/form-data; boundary="
    )
    const bytes = route.request().postDataBuffer()!.toString("utf8")
    expect(bytes).toContain('"ticket":"upload-review"')
    expect(bytes).toContain('"decision":"approve"')
    expect(bytes).toContain("Customer reference ABC")
    approvedUploads++
    return route.fulfill({
      json: { status: "completed", message: "Reference saved to Files." },
    })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    const connectionNumber = ++connections
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      if (event.type === "conversation_initiation_client_data")
        socket.send(
          JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: connectionNumber === 1 ? "conv-one" : "conv-two",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        )
      if (event.type === "multimodal_message") {
        multimodal = event
        multimodalCount++
      }
      if (event.type === "contextual_update") {
        contextual.push(String(event.text))
        if (String(event.text).includes("historical_task_references"))
          socket.send(
            JSON.stringify({
              type: "client_tool_call",
              client_tool_call: {
                tool_name: "execute_genesy_tool",
                tool_call_id: "carried-status",
                parameters: {
                  operation_id: "assistant_task_status",
                  arguments_json: JSON.stringify({
                    actionId: "0193f6b0-1c2d-7e3f-8a9b-dddddddddddd",
                  }),
                },
              },
            })
          )
      }
      if (
        event.type === "client_tool_result" &&
        event.tool_call_id === "carried-status"
      )
        carriedStatus = JSON.parse(event.result)
      if (event.type === "user_message")
        socket.send(
          JSON.stringify({
            type: "client_tool_call",
            client_tool_call: {
              tool_name: "execute_genesy_tool",
              tool_call_id: "file-upload",
              parameters: {
                operation_id: "files_create",
                arguments_json: JSON.stringify({
                  body: { document_name: "Reference", public: false },
                  attachments: { upload: ["local-1"] },
                }),
              },
            },
          })
        )
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: "Use text instead", exact: true })
    .click()
  await expect(
    page.getByRole("textbox", { name: "Message the assistant" })
  ).toBeVisible()
  await page.getByLabel("Choose reference files").setInputFiles({
    name: "reference.pdf",
    mimeType: "application/pdf",
    buffer: referencePdf(),
  })
  await expect(
    page.getByRole("list", { name: "Session references" })
  ).toContainText("Selected · not sent")
  await page
    .getByRole("textbox", { name: "Message the assistant" })
    .fill("Look at this reference")
  await page.getByRole("button", { name: "Send", exact: true }).click()
  await expect.poll(() => multimodal).toBeTruthy()
  await expect(
    page.getByRole("list", { name: "Session references" })
  ).toContainText("Shared · inspection not verified")
  expect(contextual.join("\n")).toContain("local-1")
  expect(contextual.join("\n")).toContain("provider-1")
  expect(contextual.join("\n")).not.toContain("reference.pdf")
  await page
    .getByRole("textbox", { name: "Message the assistant" })
    .fill("Save this reference to Files")
  await page.getByRole("button", { name: "Send", exact: true }).click()
  await expect(
    page.getByRole("dialog", { name: "Review action" })
  ).toContainText("Save reference to Files")
  expect(approvedUploads).toBe(0)
  await page
    .getByRole("button", { name: "Approve action", exact: true })
    .click()
  await expect.poll(() => approvedUploads).toBe(1)
  await expect(page.getByRole("log")).toContainText(
    en.voice.taskStates.completed
  )
  await page
    .getByRole("button", { name: en.voice.closeAssistant, exact: true })
    .click()
  await openMenu(page)
  await page
    .getByRole("navigation", { name: en.nav.label, exact: true })
    .getByRole("link", { name: "Orders", exact: true })
    .click()
  await expect(
    page.getByRole("button", { name: "End conversation", exact: true })
  ).toBeVisible()
  await page
    .getByRole("link", { name: "Open conversation", exact: true })
    .click()
  await expect(page.getByRole("log")).toContainText("Look at this reference")
  await expect(
    page.getByRole("list", { name: "Session references" })
  ).toContainText("reference.pdf")
  expect(starts).toBe(1)
  await page.getByRole("button", { name: "Enable voice", exact: true }).click()
  await expect(
    page.getByRole("button", { name: "Mute", exact: true })
  ).toBeVisible()
  expect(starts).toBe(2)
  expect(connections).toBe(2)
  expect(approvedUploads).toBe(1)
  expect(multimodalCount).toBe(1)
  await expect(page.getByRole("log")).toContainText("Look at this reference")
  await expect(page.getByRole("log")).toContainText(
    en.voice.taskStates.completed
  )
  await expect(
    page.getByRole("list", { name: "Session references" })
  ).toContainText("Selected · not sent")
  await expect
    .poll(() =>
      contextual.some((text) => text.includes("historical_task_references"))
    )
    .toBe(true)
  const history = contextual.find((text) =>
    text.includes("historical_task_references")
  )!
  expect(history).toContain("0193f6b0-1c2d-7e3f-8a9b-dddddddddddd")
  await expect
    .poll(() => carriedStatus)
    .toMatchObject({
      status: "completed",
      message: "Verified original saved file.",
    })
  for (const privateValue of [
    "reference.pdf",
    "Look at this reference",
    "upload-review",
    "654321",
    "provider-1",
    "session-one",
  ])
    expect(history).not.toContain(privateValue)
  await page.getByRole("button", { name: "Send", exact: true }).click()
  await expect
    .poll(() => multimodal)
    .toMatchObject({ files: [{ type: "file_input", file_id: "provider-2" }] })
  expect(uploads).toBe(2)
  expect(approvedUploads).toBe(1)
  await expectFits(page)
  await page
    .getByRole("button", { name: "End conversation", exact: true })
    .last()
    .click()
})

test("secure handoff closes conversation before collecting credentials", async ({
  page,
  api: _api,
}) => {
  void _api
  let ended = false
  const outgoing: string[] = []
  let submitted: unknown
  await page.route("**/api/voice/session", (route) => {
    if (
      route.request().method() === "POST" &&
      route.request().postDataJSON()?.command === "heartbeat"
    )
      return route.fulfill({ json: { status: "active" } })
    if (route.request().method() === "DELETE") {
      ended = true
      return route.fulfill({ json: { status: "ended" } })
    }
    return route.fulfill({
      json: {
        sessionId: "secure-session",
        conversationId: "secure-conv",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    socket.onMessage((raw) => {
      outgoing.push(String(raw))
      if (JSON.parse(String(raw)).type === "user_message")
        socket.send(
          JSON.stringify({
            type: "client_tool_call",
            client_tool_call: {
              tool_name: "execute_genesy_tool",
              tool_call_id: "secure-call",
              parameters: {
                operation_id: "email_profiles_create",
                arguments_json: "{}",
              },
            },
          })
        )
      if (
        JSON.parse(String(raw)).type === "conversation_initiation_client_data"
      )
        socket.send(
          JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: "secure-conv",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        )
    })
  })
  await page.route("**/api/voice/tools", (route) =>
    route.fulfill({
      json: {
        status: "secure_ui",
        handoff: {
          route: "/voice/secure?operation=email_profiles_create",
          operationId: "email_profiles_create",
        },
      },
    })
  )
  await page.route("**/api/voice/secure", (route) => {
    const body = route.request().postDataJSON()
    expect(ended).toBe(true)
    if (body.command === "describe")
      return route.fulfill({
        json: {
          id: "email_profiles_create",
          summary: "Create email connection",
          parameters: [],
          body: {
            type: "object",
            required: ["name", "password"],
            properties: {
              name: { type: "string", title: "Connection name" },
              password: { type: "string", title: "Connection password" },
            },
          },
        },
      })
    submitted = body.arguments
    return route.fulfill({
      json: { status: "completed", data: { id: "connection-one" } },
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: "Use text instead", exact: true })
    .click()
  await expect(
    page.getByRole("textbox", { name: "Message the assistant" })
  ).toBeVisible()
  await page
    .getByRole("textbox", { name: "Message the assistant" })
    .fill("Create an email connection")
  await page.getByRole("button", { name: "Send", exact: true }).click()
  await page
    .getByRole("link", { name: "Open secure form", exact: true })
    .click()
  await page
    .getByLabel("Connection name", { exact: false })
    .fill("Office email")
  await page
    .getByLabel("Connection password", { exact: false })
    .fill("ui-only-value")
  await page.getByRole("button", { name: "Submit action", exact: true }).click()
  await expect
    .poll(() => submitted)
    .toEqual({
      path: {},
      query: {},
      body: { name: "Office email", password: "ui-only-value" },
    })
  expect(outgoing.join("\n")).not.toContain("ui-only-value")
  await expect(
    page.getByText('"id": "connection-one"', { exact: false })
  ).toBeVisible()
  await expectFits(page)
})

test("mismatched provider conversation cannot open business capabilities", async ({
  page,
  api: _api,
}) => {
  void _api
  let closed = false
  let tools = 0
  await page.route("**/api/voice/session", (route) => {
    if (
      route.request().method() === "POST" &&
      route.request().postDataJSON()?.command === "heartbeat"
    )
      return route.fulfill({ json: { status: "active" } })
    if (route.request().method() === "DELETE") {
      closed = true
      return route.fulfill({ json: { status: "ended" } })
    }
    return route.fulfill({
      json: {
        sessionId: "bound-session",
        conversationId: "trusted-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.route("**/api/voice/tools", (route) => {
    tools++
    return route.fulfill({ json: { status: "failed" } })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    socket.onMessage((raw) => {
      if (
        JSON.parse(String(raw)).type === "conversation_initiation_client_data"
      )
        socket.send(
          JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: "wrong-conversation",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        )
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: "Use text instead", exact: true })
    .click()
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "conversation identity could not be verified" })
  ).toBeVisible()
  await expect.poll(() => closed).toBe(true)
  await expect(
    page.getByRole("textbox", { name: "Message the assistant" })
  ).toHaveCount(0)
  expect(tools).toBe(0)
  await expectFits(page)
})

test.describe("heartbeat authorization", () => {
  test.use({
    allow: { responses: [{ url: "/api/voice/session", status: 410 }] },
  })
  test("ends capture and controls when the bound session is revoked", async ({
    page,
    api: _api,
  }) => {
    void _api
    let beats = 0
    let closed = false
    await page.clock.install()
    await page.route("**/api/voice/session", (route) => {
      if (route.request().method() === "DELETE") {
        closed = true
        return route.fulfill({ json: { status: "ended" } })
      }
      if (route.request().postDataJSON()?.command === "heartbeat") {
        beats++
        return route.fulfill({
          status: beats === 1 ? 200 : 410,
          json: { status: beats === 1 ? "active" : "expired" },
        })
      }
      return route.fulfill({
        json: {
          sessionId: "heartbeat-session",
          conversationId: "heartbeat-conversation",
          signedUrl:
            "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
        },
      })
    })
    await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
      socket.onMessage((raw) => {
        if (
          JSON.parse(String(raw)).type === "conversation_initiation_client_data"
        )
          socket.send(
            JSON.stringify({
              type: "conversation_initiation_metadata",
              conversation_initiation_metadata_event: {
                conversation_id: "heartbeat-conversation",
                agent_output_audio_format: "pcm_16000",
                user_input_audio_format: "pcm_16000",
              },
            })
          )
      })
    })
    await page.goto("/voice")
    await page
      .getByRole("button", { name: "Use text instead", exact: true })
      .click()
    await expect(
      page.getByRole("textbox", { name: "Message the assistant" })
    ).toBeVisible()
    await page.clock.fastForward(30000)
    await expect.poll(() => beats).toBe(1)
    await expect(
      page.getByRole("textbox", { name: "Message the assistant" })
    ).toBeVisible()
    await page.clock.fastForward(30000)
    await expect.poll(() => closed).toBe(true)
    await expect(
      page.getByRole("textbox", { name: "Message the assistant" })
    ).toHaveCount(0)
    await expect(
      page.getByRole("alert").filter({ hasText: "no longer authorized" })
    ).toBeVisible()
    await page.clock.fastForward(60000)
    expect(beats).toBe(2)
  })
})

test("only the final user transcript naming the current action code can approve", async ({
  page,
  api: _api,
}) => {
  void _api
  let prepare = 0
  let commits = 0
  let spokenOutcome: unknown
  let emit: (event: Record<string, unknown>) => void = () => {
    throw new Error("conversation not connected")
  }
  const codes = ["123456", "654321", "000001", "222222", "333333"]
  let releaseApproval = () => {}
  const approvalGate = new Promise<void>((resolve) => {
    releaseApproval = resolve
  })
  await page.route("**/api/voice/session", (route) => {
    if (route.request().method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })
    if (route.request().postDataJSON()?.command === "heartbeat")
      return route.fulfill({ json: { status: "active" } })
    return route.fulfill({
      json: {
        sessionId: "spoken-session",
        conversationId: "spoken-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.route("**/api/voice/tools", (route) => {
    const body = route.request().postDataJSON()
    if (body.command === "status")
      return route.fulfill({
        json: body.actionId
          ? { status: "awaiting_review", data: { actionId: body.actionId } }
          : { status: "completed", data: [] },
      })
    const code = codes[prepare++]!
    return route.fulfill({
      json: {
        status: "requires_confirmation",
        data: {
          actionId: `0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaa${prepare}`,
        },
        review: {
          ticket: `spoken-ticket-${prepare}`,
          summary: "Create a draft order",
          target: "Ilundo Honey Co.",
          details: { reference: "Exact reviewed order" },
          confirmationCode: code,
          confirmationPhrase: `Confirm action ${code}`,
          expiresAt: new Date(Date.now() + 120000).toISOString(),
        },
      },
    })
  })
  await page.route("**/api/voice/confirm", async (route) => {
    const body = route.request().postDataJSON()
    if (body.decision === "cancel")
      return route.fulfill({ json: { status: "cancelled" } })
    expect(body).toMatchObject({
      sessionId: "spoken-session",
      workspaceId: "w1",
      ticket: `spoken-ticket-${prepare}`,
      decision: "approve",
      method: "speech",
      eventId: prepare === 3 ? "12" : "20",
      confirmationCode: prepare === 3 ? "000001" : "333333",
      utterance: `Confirm action ${prepare === 3 ? "000001" : "333333"}`,
      locale: "en",
    })
    commits++
    if (prepare === 3) await approvalGate
    return route.fulfill({
      json: {
        status: prepare === 3 ? "completed" : "unknown",
        message:
          prepare === 3
            ? "The exact reviewed order was created."
            : "Check the order before repeating.",
      },
    })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    emit = (event) => socket.send(JSON.stringify(event))
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      if (["client_tool_result", "contextual_update"].includes(event.type)) {
        for (const code of codes) expect(String(raw)).not.toContain(code)
        expect(String(raw)).not.toContain("confirmationCode")
        expect(String(raw)).not.toContain("confirmationPhrase")
      }
      if (event.type === "conversation_initiation_client_data")
        emit({
          type: "conversation_initiation_metadata",
          conversation_initiation_metadata_event: {
            conversation_id: "spoken-conversation",
            agent_output_audio_format: "pcm_16000",
            user_input_audio_format: "pcm_16000",
          },
        })
      if (event.type === "user_message")
        emit({
          type: "client_tool_call",
          client_tool_call: {
            tool_name: "execute_genesy_tool",
            tool_call_id: `prepare-${prepare}`,
            parameters: {
              operation_id: "orders_create",
              arguments_json:
                '{"body":{"reference":"Exact reviewed order","lines":[{"product_id":"0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaaa","quantity":"1"}]}}',
            },
          },
        })
      if (event.type === "client_tool_result") {
        const result = JSON.parse(event.result)
        if (String(event.tool_call_id).startsWith("spoken-watch")) {
          if (["completed", "unknown"].includes(result.status)) {
            spokenOutcome = result
            emit({
              type: "agent_response",
              agent_response_event: {
                agent_response: result.message,
                event_id: 200,
              },
            })
          }
          return
        }
        expect(result.confirmationCode).toBeUndefined()
        expect(result.confirmationPhrase).toBeUndefined()
        expect(Object.keys(result).sort()).toEqual([
          "actionId",
          "message",
          "status",
        ])
        expect(event.result).not.toContain("spoken-ticket")
        expect(event.result).not.toContain("Exact reviewed order")
        // Even an agent response containing the challenge cannot approve an action.
        emit({
          type: "agent_response",
          agent_response_event: {
            agent_response: `Confirm action ${codes[prepare - 1]}`,
            event_id: 100 + prepare,
          },
        })
        emit({
          type: "client_tool_call",
          client_tool_call: {
            tool_name: "execute_genesy_tool",
            tool_call_id: `spoken-watch-${prepare}`,
            parameters: {
              operation_id: "assistant_task_status",
              arguments_json: JSON.stringify({
                actionId: result.actionId,
                wait: true,
              }),
            },
          },
        })
      }
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: "Use text instead", exact: true })
    .click()
  const ask = async () => {
    await page
      .getByRole("textbox", { name: "Message the assistant" })
      .fill("Create a draft order")
    await page.getByRole("button", { name: "Send", exact: true }).click()
    await expect(
      page.getByRole("dialog", { name: "Review action" })
    ).toBeVisible()
  }
  await ask()
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toContainText("Confirm action 123456")
  expect(commits).toBe(0)
  emit({
    type: "user_transcript",
    user_transcription_event: { user_transcript: "yes", event_id: 10 },
  })
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toHaveCount(0)
  expect(commits).toBe(0)
  await ask()
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toContainText("Confirm action 654321")
  emit({
    type: "user_transcript",
    user_transcription_event: {
      user_transcript: "Confirm action 123456",
      event_id: 11,
    },
  })
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toHaveCount(0)
  expect(commits).toBe(0)
  await ask()
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toContainText("Confirm action 000001")
  const final = {
    type: "user_transcript",
    user_transcription_event: {
      user_transcript: "Confirm action 000001",
      event_id: 12,
    },
  }
  emit(final)
  emit(final)
  await expect.poll(() => commits).toBe(1)
  emit({
    ...final,
    user_transcription_event: {
      ...final.user_transcription_event,
      event_id: 13,
    },
  })
  emit({
    type: "agent_response",
    agent_response_event: {
      agent_response: "Confirm action 000001",
      event_id: 300,
    },
  })
  await expect(
    page.getByRole("dialog", { name: "Review action" })
  ).toBeVisible()
  expect(commits).toBe(1)
  releaseApproval()
  await expect
    .poll(() => spokenOutcome)
    .toMatchObject({
      status: "completed",
      message: "The exact reviewed order was created.",
    })
  await expect(page.getByRole("log")).toContainText(
    "The exact reviewed order was created."
  )
  emit({
    ...final,
    user_transcription_event: {
      ...final.user_transcription_event,
      event_id: 14,
    },
  })
  await ask()
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toContainText("Confirm action 222222")
  emit({
    ...final,
    user_transcription_event: {
      ...final.user_transcription_event,
      event_id: 15,
    },
  })
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toHaveCount(0)
  expect(commits).toBe(1)
  await ask()
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toContainText("Confirm action 333333")
  const uncertain = {
    type: "user_transcript",
    user_transcription_event: {
      user_transcript: "Confirm action 333333",
      event_id: 20,
    },
  }
  emit(uncertain)
  await expect.poll(() => spokenOutcome).toMatchObject({ status: "unknown" })
  emit(uncertain)
  emit({
    ...uncertain,
    user_transcription_event: {
      ...uncertain.user_transcription_event,
      event_id: 21,
    },
  })
  emit({
    type: "agent_response",
    agent_response_event: {
      agent_response: "Confirm action 333333",
      event_id: 301,
    },
  })
  await expect(
    page.getByRole("dialog", { name: en.voice.review, exact: true })
  ).toHaveCount(0)
  expect(commits).toBe(2)
  await expectFits(page)
  await page
    .getByRole("button", { name: "End conversation", exact: true })
    .click()
})
