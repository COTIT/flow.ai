// Fake Office voice server for Flow's conversation specs (tests/flow/chat-*.spec.ts). Flow's
// built proxy forwards /api/voice/* here (VOICE_UPSTREAM_URL is the mock origin), so these
// answers prove the real browser -> Flow -> "Office" path. Specs override single answers with
// page.route when a scenario needs it. Everything here is simulated: no ElevenLabs, no Office.

export const FLOW_SIGNED_URL =
  "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=flow-e2e"
export const FLOW_ORDER = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"

const orders = {
  status: "completed",
  data: {},
  view: {
    kind: "records",
    resource: "orders",
    total: 7,
    href: "/orders",
    rows: [
      {
        id: FLOW_ORDER,
        label: "ORD-0042",
        detail: "Neema Shop",
        status: "open",
        money: { minor: 125050, decimals: 2, currency: "USD" },
        stock: null,
        href: `/orders/${FLOW_ORDER}`,
      },
    ],
  },
}

const sales = {
  status: "completed",
  data: {},
  view: {
    kind: "sales",
    from: "2026-11-09T00:00:00+03:00",
    to: "2026-11-10T00:00:00+03:00",
    source: "till",
    rows: [
      {
        currency: "TZS",
        decimals: 0,
        gross: 1250000,
        net: null,
        vat: null,
        refunded: 0,
        orders: 42,
      },
    ],
  },
}

const review = () => ({
  status: "requires_confirmation",
  data: { actionId: "0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaaa" },
  review: {
    ticket: "flow-review-ticket",
    summary: "Create order for Neema Shop",
    confirmationCode: "135790",
    confirmationPhrase: "Confirm action 135790",
    target: "Current workspace",
    details: { reference: "Flow e2e order", customer: "Neema Shop" },
    expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
  },
})

const page = (name, href) => ({
  status: "completed",
  data: { page: name },
  links: [{ label: name, href }],
  narration: name,
})

const secure = {
  status: "handoff",
  message: "This needs the secure form.",
  handoff: {
    route: "/voice/secure?operation=app_keys_create",
    operationId: "app_keys_create",
  },
}

/** Answers one Flow voice request, or returns false so the caller carries on. */
export async function flowVoice(req, res, path, send, readJson) {
  if (!path.startsWith("/api/voice/")) return false
  const route = path.slice("/api/voice/".length)
  if (route === "session" && req.method === "POST") {
    const body = await readJson(req)
    if (body.command === "dropped")
      return (send(res, 200, { status: "recorded" }), true)
    if (body.command === "heartbeat")
      return (send(res, 200, { status: "active" }), true)
    return (
      send(res, 200, {
        sessionId: "flow-session",
        conversationId: "flow-conversation",
        signedUrl: FLOW_SIGNED_URL,
      }),
      true
    )
  }
  if (route === "tools" && req.method === "POST") {
    const body = await readJson(req).catch(() => ({}))
    if (body.command !== "execute")
      return (send(res, 200, { status: "completed", data: [] }), true)
    const answer =
      body.operationId === "orders_list"
        ? orders
        : body.operationId === "orders_sales_summary"
          ? sales
          : body.operationId === "orders_create"
            ? review()
            : body.operationId === "office_page_open"
              ? body.arguments?.page === "storefront"
                ? page("Storefront design", "/settings/storefront")
                : page("Orders", "/orders")
              : body.operationId === "app_keys_create"
                ? secure
                : { status: "failed", message: "Unknown e2e operation." }
    return (send(res, 200, answer), true)
  }
  if (route === "confirm" && req.method === "POST") {
    const body = await readJson(req)
    return (
      send(
        res,
        200,
        body.decision === "approve"
          ? {
              status: "completed",
              message: "Order created.",
              links: [{ label: "ORD-0043", href: `/orders/${FLOW_ORDER}` }],
            }
          : { status: "cancelled", message: "Nothing was changed." }
      ),
      true
    )
  }
  if (route === "attachments" && req.method === "POST") {
    req.resume()
    return (
      send(res, 200, {
        id: "flow-file-1",
        fileId: "provider-file-1",
        status: "uploaded",
      }),
      true
    )
  }
  if (route === "attachments" && req.method === "DELETE")
    return (send(res, 200, { status: "deleted" }), true)
  return false
}
