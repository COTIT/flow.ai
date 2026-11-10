import { expect, expectFits, test } from "./fake-api"
import officeEn from "../../../../apps/office/messages/en.json" with { type: "json" }
import officeNl from "../../../../apps/office/messages/nl.json" with { type: "json" }
import officeSw from "../../../../apps/office/messages/sw.json" with { type: "json" }
import voiceEn from "../../../../packages/i18n/messages/voice/en.json" with { type: "json" }
import voiceNl from "../../../../packages/i18n/messages/voice/nl.json" with { type: "json" }
import voiceSw from "../../../../packages/i18n/messages/voice/sw.json" with { type: "json" }

// Office hosts the shared voice texts next to its own.
const en = { ...voiceEn, ...officeEn }
const nl = { ...voiceNl, ...officeNl }
const sw = { ...voiceSw, ...officeSw }

for (const { locale, messages, money, request, summary, phrase } of [
  {
    locale: "en",
    messages: en,
    money: "€12.50",
    request: "Request a refund of 12.50 EUR for this order",
    summary: "Request an order refund",
    phrase: "Confirm action 864209",
  },
  {
    locale: "nl",
    messages: nl,
    money: "€\u00a012,50",
    request: "Vraag een terugbetaling van 12,50 EUR voor deze bestelling aan",
    summary: "Een terugbetaling aanvragen",
    phrase: "Bevestig actie 864209",
  },
  {
    locale: "sw",
    messages: sw,
    money: "€\u00a012.50",
    request: "Omba kurejeshewa 12.50 EUR kwa agizo hili",
    summary: "Omba marejesho ya agizo",
    phrase: "Thibitisha kitendo 864209",
  },
]) {
  test(`${locale} refund keeps exact money on private review and reports queued without an unlabelled minor amount`, async ({
    page,
    api: _api,
    baseURL,
  }) => {
    void _api
    await page.context().addCookies([
      { name: "NEXT_LOCALE", value: locale, url: baseURL! },
    ])
    const actionId = "01950000-0000-7000-8000-000000000001"
    const orderId = "01950000-0000-7000-8000-000000000002"
    const refundId = "01950000-0000-7000-8000-000000000003"
    const resultMessage =
      "Refund requested. Provider settlement is not yet confirmed."
    // Exact projected BFF contract: native 1250 has no currency metadata here.
    const queued = {
      status: "queued",
      message: resultMessage,
      data: { data: { refund_id: refundId, status: "creating" } },
    }
    let approvals = 0
    let userMessages = 0
    let continuation: unknown
    let preparation: unknown
    const sdkPayloads: string[] = []
    await page.route("**/api/voice/session", (route) => {
      if (route.request().method() === "DELETE")
        return route.fulfill({ json: { status: "ended" } })
      if (route.request().postDataJSON()?.command === "heartbeat")
        return route.fulfill({ json: { status: "active" } })
      expect(route.request().postDataJSON()).toMatchObject({
        locale,
        textOnly: true,
      })
      return route.fulfill({
        json: {
          sessionId: "refund-session",
          conversationId: "refund-conversation",
          signedUrl:
            "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
        },
      })
    })
    await page.route("**/api/voice/tools", (route) => {
      const body = route.request().postDataJSON()
      expect(body).toMatchObject({
        workspaceId: "w1",
        sessionId: "refund-session",
      })
      expect(route.request().headers().authorization).toBe("Bearer e2e-token")
      if (body.command === "status")
        return route.fulfill({
          json: body.actionId
            ? approvals
              ? queued
              : { status: "awaiting_review", data: { actionId } }
            : { status: "completed", data: [] },
        })
      expect(body).toMatchObject({
        operationId: "orders_refund",
        arguments: { path: { order: orderId }, body: { amount: "12.50" } },
      })
      return route.fulfill({
        json: {
          status: "requires_confirmation",
          data: { actionId },
          review: {
            ticket: "private-refund-ticket",
            confirmationCode: "864209",
            confirmationPhrase: phrase,
            summary,
            target: "orders",
            details: {
              workspace: "w1",
              target: { order: orderId },
              current: {
                kind: "order",
                locked: { currency: "EUR", decimals: 2, refundable: money },
              },
              changes: { amount: "12.50" },
            },
            expiresAt: new Date(Date.now() + 120000).toISOString(),
          },
        },
      })
    })
    await page.route("**/api/voice/confirm", (route) => {
      expect(route.request().postDataJSON()).toMatchObject({
        workspaceId: "w1",
        sessionId: "refund-session",
        ticket: "private-refund-ticket",
        decision: "approve",
        method: "click",
      })
      approvals++
      return route.fulfill({ json: queued })
    })
    await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
      socket.onMessage((raw) => {
        const event = JSON.parse(String(raw))
        if (event.type === "conversation_initiation_client_data") {
          expect(event.conversation_config_override.agent.language).toBe(locale)
          socket.send(JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: "refund-conversation",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          }))
        }
        if (event.type === "user_message") {
          userMessages++
          socket.send(JSON.stringify({
            type: "client_tool_call",
            client_tool_call: {
              tool_name: "execute_genesy_tool",
              tool_call_id: "refund-prepare",
              parameters: {
                operation_id: "orders_refund",
                arguments_json: JSON.stringify({
                  path: { order: orderId },
                  body: { amount: "12.50" },
                }),
              },
            },
          }))
        }
        if (["client_tool_result", "contextual_update"].includes(event.type))
          sdkPayloads.push(String(raw))
        if (event.type !== "client_tool_result") return
        const result = JSON.parse(event.result)
        if (event.tool_call_id === "refund-prepare") {
          preparation = result
          socket.send(JSON.stringify({
            type: "client_tool_call",
            client_tool_call: {
              tool_name: "execute_genesy_tool",
              tool_call_id: "refund-watch",
              parameters: {
                operation_id: "assistant_task_status",
                arguments_json: JSON.stringify({ actionId, wait: true }),
              },
            },
          }))
        } else if (event.tool_call_id.startsWith("refund-watch")) {
          if (result.status === "running") {
            socket.send(JSON.stringify({
              type: "client_tool_call",
              client_tool_call: {
                tool_name: "execute_genesy_tool",
                tool_call_id: "refund-watch-again",
                parameters: {
                  operation_id: "assistant_task_status",
                  arguments_json: JSON.stringify({ actionId, wait: true }),
                },
              },
            }))
            return
          }
          continuation = result
          socket.send(JSON.stringify({
            type: "agent_response",
            agent_response_event: { agent_response: resultMessage },
          }))
        }
      })
    })
    await page.goto("/voice")
    await page.getByRole("button", {
      name: messages.voice.textInstead,
      exact: true,
    }).click()
    await page.getByLabel(messages.voice.message).fill(request)
    await page.getByRole("button", { name: messages.voice.send, exact: true }).click()
    const review = page.getByRole("dialog", { name: messages.voice.review })
    await expect(review).toBeVisible()
    await expect(review).toContainText(money)
    await expect(review).toContainText("EUR")
    await expect(review).not.toContainText(/\b1250\b/)
    await expect.poll(() => preparation).toMatchObject({
      status: "requires_confirmation",
      actionId,
    })
    expect(approvals).toBe(0)
    await expectFits(page)
    await review.getByRole("button", {
      name: messages.voice.approve,
      exact: true,
    }).click()
    await expect(review).toHaveCount(0)
    const task = page.getByRole("dialog", { name: messages.voice.title, exact: true })
    await expect(page.locator("#voice-task")).toHaveCount(0)
    await expect(task).not.toContainText(refundId)
    await expect.poll(() => continuation).toEqual(queued)
    await expect(page.getByRole("log")).toContainText(resultMessage)
    await expect(page.getByRole("log")).not.toContainText(messages.voice.taskStates.completed)
    expect(approvals).toBe(1)
    expect(userMessages).toBe(1)
    expect(sdkPayloads.join("\n")).toContain("assistant_progress")
    expect(sdkPayloads.join("\n")).not.toMatch(/\b1250\b/)
    expect(sdkPayloads.join("\n")).not.toContain("private-refund-ticket")
    expect(sdkPayloads.join("\n")).not.toContain("864209")
    await expectFits(page)
    await page.getByRole("button", { name: messages.voice.end, exact: true }).click()
  })
}
