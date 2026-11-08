import { expect, expectFits, test } from "./fake-api"
import en from "../../../../apps/office/messages/en.json" with { type: "json" }

// Review card data as the Office BFF builds it from API review facts (lib/voice/actions.ts).
const reviews = [
  {
    name: "delete names the customer and lists only what the API reports",
    operationId: "relations_delete",
    request: "Delete the customer Kendall Moreau and everything about them.",
    details: {
      workspace: "w1",
      target: { relation: "01950000-0000-7000-8000-000000000002" },
      record: { type: "customer", label: "Kendall Moreau" },
      impact: { effect: "archive", orders: 2, contacts: 1 },
    },
    shows: [
      "Kendall Moreau",
      "customer",
      en.voice.results.impactTitle,
      en.voice.reviewFields.orders,
    ],
  },
  {
    name: "a tenfold price change shows old and new price and a plain warning",
    operationId: "products_update",
    request: "Raise the price of AFM Stickers by 500.",
    details: {
      workspace: "w1",
      target: { product: "01950000-0000-7000-8000-000000000003" },
      record: { type: "product", label: "AFM Stickers" },
      warnings: [
        "Price goes from $0.50 to $500.50, more than 10 times the current amount. Check the amount and its unit.",
      ],
      current: {
        kind: "product",
        changes: [
          {
            field: "price",
            currency: "USD",
            before: "$0.50",
            after: "$500.50",
            change_bp: 10000000,
          },
        ],
      },
      changes: { price: "500.5" },
    },
    shows: [
      "AFM Stickers",
      "more than 10 times the current amount",
      "$0.50",
      "$500.50",
      en.voice.results.warningsTitle,
    ],
  },
]

for (const review of reviews) {
  test(review.name, async ({ page, api: _api }) => {
    void _api
    const actionId = "01950000-0000-7000-8000-000000000001"
    await page.route("**/api/voice/session", (route) => {
      if (route.request().method() === "DELETE")
        return route.fulfill({ json: { status: "ended" } })
      if (route.request().postDataJSON()?.command === "heartbeat")
        return route.fulfill({ json: { status: "active" } })
      return route.fulfill({
        json: {
          sessionId: "safety-session",
          conversationId: "safety-conversation",
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
            ? { status: "awaiting_review", data: { actionId } }
            : { status: "completed", data: [] },
        })
      return route.fulfill({
        json: {
          status: "requires_confirmation",
          data: { actionId },
          review: {
            ticket: "private-ticket",
            confirmationCode: "864209",
            confirmationPhrase: "Confirm action 864209",
            summary: review.request,
            operationId: review.operationId,
            method: review.operationId.endsWith("delete") ? "DELETE" : "PATCH",
            details: review.details,
            expiresAt: new Date(Date.now() + 120000).toISOString(),
          },
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
                conversation_id: "safety-conversation",
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
                tool_call_id: "safety-prepare",
                parameters: {
                  operation_id: review.operationId,
                  arguments_json: "{}",
                },
              },
            })
          )
      })
    })
    await page.goto("/voice")
    await page
      .getByRole("button", { name: en.voice.textInstead, exact: true })
      .click()
    await page.getByLabel(en.voice.message).fill(review.request)
    await page.getByRole("button", { name: en.voice.send, exact: true }).click()
    const card = page.getByRole("dialog", { name: en.voice.review })
    await expect(card).toBeVisible()
    for (const text of review.shows) await expect(card).toContainText(text)
    // Wording of change_bp and effect belongs to voice-rich-results' card (b517a0306), not checked here.
    await expect(card).not.toContainText("Selected reference")
    await expect(card).not.toContainText("large_change")
    await expect(card).not.toContainText("01950000")
    await expectFits(page)
  })
}
