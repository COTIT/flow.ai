import { expect, expectFits, test } from "./fake-api"
import officeEn from "../../../../apps/office/messages/en.json" with { type: "json" }
import voiceEn from "../../../../packages/i18n/messages/voice/en.json" with { type: "json" }

// Office hosts the shared voice texts next to its own.
const en = { ...voiceEn, ...officeEn }

type Review = {
  name: string
  operationId: string
  request: string
  details: Record<string, unknown>
  shows: string[]
  hides?: string[]
  /** Review fields beyond a single action's (method, batch kind and steps). */
  extra?: Record<string, unknown>
  /** A create: every row is a new value, never struck-through old → new. */
  newOnly?: boolean
}

// Review card data as the Office BFF builds it from API review facts (lib/voice/actions.ts).
const reviews: Review[] = [
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
  {
    name: "a stock transfer names both warehouses with levels from proposal time",
    operationId: "stock_transfer",
    request: "Move 20 bags of maize from the main store to the shop.",
    extra: { method: "POST" },
    details: {
      workspace: "w1",
      target: { product: "01950000-0000-7000-8000-000000000004" },
      record: { type: "product", label: "Maize 50 kg" },
      current: {
        kind: "stock_transfer",
        locked: { lot: { code: "MZ-1", status: "released" } },
        changes: [
          {
            field: "stock_from",
            label: "Main store",
            before: "120",
            after: "100",
          },
          { field: "stock_to", label: "Shop", before: "5", after: "25" },
        ],
      },
      changes: { quantity: "20", reason: "Restock shop" },
    },
    shows: ["Maize 50 kg", "Main store", "Shop", "120", "100", "25"],
    hides: ["Stock from", "stock_from"],
  },
  {
    // P3-3: the number being set leads the card, before More details.
    name: "setting stock levels shows the warehouse, old and new amount, with a plain title",
    operationId: "stock_set_levels",
    request: "Set the honey stock to 130.",
    extra: { method: "POST" },
    details: {
      workspace: "w1",
      target: { product: "01950000-0000-7000-8000-000000000005" },
      record: { type: "product", label: "Raw Honey 500g" },
      current: {
        kind: "stock",
        locked: { id: "01950000-0000-7000-8000-000000000005", slots: [] },
        changes: [{ field: "stock", label: "Main store", before: "14", after: "130" }],
      },
      changes: {
        warehouses: [{ warehouse_id: "01950000-0000-7000-8000-000000000006", stock_lvl: 130 }],
      },
    },
    shows: [
      en.voice.operationTitles.stock_set_levels,
      "Raw Honey 500g",
      `Main store · ${en.voice.reviewFields.stock}`,
      "14",
      "130",
    ],
    hides: ["Run action", en.voice.operationActions.run],
  },
  {
    name: "confirming an order shows its status change in words",
    operationId: "orders_confirm",
    request: "Confirm order 1042.",
    extra: { method: "POST" },
    details: {
      workspace: "w1",
      target: { order: "01950000-0000-7000-8000-000000000007" },
      record: { type: "order", label: "1042" },
      current: {
        kind: "order",
        changes: [{ field: "status", before: en.voice.values.draft, after: en.voice.values.open }],
      },
      changes: {},
    },
    shows: [en.voice.operationTitles.orders_confirm, en.voice.reviewFields.status, "Draft", "Open"],
  },
  {
    name: "without API rows, the request's plain values lead and ids stay hidden",
    operationId: "relations_update",
    request: "Rename Kendall's company to Mama Nzuri Ltd.",
    details: {
      workspace: "w1",
      target: { relation: "01950000-0000-7000-8000-000000000008" },
      record: { type: "customer", label: "Kendall Moreau" },
      changes: {
        company_name: "Mama Nzuri Ltd",
        country_id: "01950000-0000-7000-8000-000000000009",
        address: { city: "Arusha" },
      },
    },
    shows: [en.voice.reviewFields.company_name, "Mama Nzuri Ltd"],
    hides: ["Arusha", "country"],
    newOnly: true,
  },
  {
    name: "setting an exchange rate names the pair and shows old and new rate with the day",
    operationId: "currency_rates_create",
    request: "Set the euro to shilling rate to 3000 from 1 November.",
    details: {
      workspace: "w1",
      // actions.ts names the card's record from the facts' locked label.
      record: { type: "record", label: "EUR → TZS" },
      current: {
        kind: "currency_rate",
        locked: {
          label: "EUR → TZS",
          from: {
            id: "01950000-0000-7000-8000-0000000000e1",
            name: "EUR",
            code: "EUR",
            decimals: 2,
          },
          to: {
            id: "01950000-0000-7000-8000-0000000000e2",
            name: "TZS",
            code: "TZS",
            decimals: 0,
          },
          previous: { rate: "2900", effective_on: "2026-10-01" },
        },
        changes: [
          { field: "rate", before: "2900", after: "3000" },
          { field: "effective_on", before: "2026-10-01", after: "2026-11-01" },
        ],
        effective_on: "2026-11-01",
        note: null,
        source: "workspace",
      },
      changes: {
        from_currency_id: "01950000-0000-7000-8000-0000000000e1",
        to_currency_id: "01950000-0000-7000-8000-0000000000e2",
        rate: "3000",
        effective_on: "2026-11-01",
      },
    },
    shows: [
      en.voice.resources.currency_rates,
      "EUR → TZS",
      "2900",
      "3000",
      "2026-10-01",
      "2026-11-01",
      en.voice.reviewFields.effective_on,
    ],
  },
  {
    name: "creating a currency names it and lists code, symbol and decimals as new values",
    operationId: "currencies_create",
    request: "Add the euro as a currency with two decimals.",
    extra: { method: "POST" },
    newOnly: true,
    details: {
      workspace: "w1",
      target: {},
      // actions.ts names the record from the body-built facts (review-facts.ts createdFacts).
      record: { type: "record", label: "Euro" },
      current: {
        kind: "currency",
        locked: { label: "Euro" },
        changes: [
          { field: "code", before: null, after: "EUR" },
          { field: "name", before: null, after: "Euro" },
          { field: "symbol", before: null, after: "€" },
          { field: "decimals", before: null, after: "2" },
        ],
      },
      changes: {
        asset_type: "EUR",
        name: "Euro",
        symbol_prefix: "€",
        decimals: 2,
        default_currency: false,
        active: true,
      },
    },
    shows: [
      "Euro",
      "EUR",
      "€",
      en.voice.reviewFields.code,
      en.voice.reviewFields.symbol,
      en.voice.reviewFields.decimals,
    ],
    hides: [en.voice.results.nameUnavailable],
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
            ...review.extra,
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
    for (const text of review.hides ?? [])
      await expect(card).not.toContainText(text)
    if (review.newOnly)
      await expect(card.locator("[data-review-diff] del")).toHaveCount(0)
    await expectFits(page)
  })
}
