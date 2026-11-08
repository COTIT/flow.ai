import { afterEach, expect, it, vi } from "vitest"
import { authorizeVoice, callBackend } from "./server"

afterEach(() => vi.unstubAllGlobals())

it.each([
  ["creating", "queued"],
  ["pending", "queued"],
  ["refunded", "unknown"],
  ["failed", "unknown"],
  ["unknown", "unknown"],
  [undefined, "unknown"],
] as const)(
  "keeps refund %s distinct from settled money",
  async (status, outcome) => {
    const payload = {
      refund_id: "01950000-0000-7000-8000-000000000003",
      amount: 1250,
      status,
    }
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => Response.json({ data: payload }, { status: 201 }))
    )
    const result = await callBackend(
      {
        operationId: "orders_refund",
        summary: "Refund order",
        method: "POST",
        path: "/workspace/w1/orders/o1/refunds",
        write: true,
        idempotent: false,
      },
      {
        token: "staff",
        workspaceId: "w1",
        apiUrl: "https://api.test/v1",
        permissions: [],
        sessionId: "s1",
      },
      "a1"
    )
    expect(result.status).toBe(outcome)
    if (outcome === "queued") {
      expect(result.data).toEqual({
        data: { refund_id: payload.refund_id, status },
      })
      expect(result.message).toContain("not yet confirmed")
    }
  }
)

async function generationOutcome(receipt: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => Response.json({ data: [], receipt }, { status: 201 }))
  )
  return callBackend(
    {
      operationId: "product_variants_generate",
      summary: "Generate variants",
      method: "POST",
      path: "/workspace/w1/products/p1/variants/generate",
      write: true,
      idempotent: false,
    },
    {
      token: "staff",
      workspaceId: "w1",
      apiUrl: "https://api.test/v1",
      permissions: [],
      sessionId: "s1",
    },
    "a1"
  )
}

it.each([
  [{ status: "partial", requested: 2, created: 0, skipped: 2 }, "partial"],
  [{ status: "applied", requested: 0, created: 0, skipped: 0 }, "completed"],
] as const)(
  "projects authoritative variant generation receipt %j",
  async (receipt, status) => {
    const result = await generationOutcome(receipt)
    expect(result.status).toBe(status)
    expect(result.data).toEqual({ data: [], receipt })
  }
)

it.each([
  undefined,
  { status: "applied", requested: 2, created: 1, skipped: 1 },
  { status: "partial", requested: 1, created: 1, skipped: 0 },
  { status: "partial", requested: 3, created: 1, skipped: 1 },
  { status: "partial", requested: 1, created: -1, skipped: 2 },
  { status: "partial", requested: 2, created: 0.5, skipped: 1.5 },
  { status: "partial", requested: 501, created: 0, skipped: 501 },
  { status: "partial", requested: 2, created: 1, skipped: 1 },
])(
  "keeps missing or inconsistent variant generation receipt unknown: %j",
  async (receipt) => {
    expect((await generationOutcome(receipt)).status).toBe("unknown")
  }
)

it.each(["teams_create", "teams_set_permissions"])(
  "reports unapplied permissions for %s without claiming completion",
  async (operationId) => {
    const permission_changes = {
      status: "partial",
      applied: ["relation.list"],
      not_applied: ["workspace_admin"],
      ignored_count: 0,
    }
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        Response.json(
          operationId === "teams_create"
            ? {
                data: {
                  team_id: "01950000-0000-7000-8000-000000000003",
                  permission_changes,
                },
              }
            : { message: "ok", permission_changes },
          { status: operationId === "teams_create" ? 201 : 200 }
        )
      )
    )
    const result = await callBackend(
      {
        operationId,
        summary: "Change team permissions",
        method: "POST",
        path: "/workspace/w1/users/teams",
        write: true,
        idempotent: false,
      },
      {
        token: "staff",
        workspaceId: "w1",
        apiUrl: "https://api.test/v1",
        permissions: [],
        sessionId: "s1",
      },
      "a1"
    )
    expect(result.status).toBe("partial")
    expect(result.data).toMatchObject({ permission_changes })
    if (operationId === "teams_create")
      expect(result.data).toMatchObject({
        team_id: "01950000-0000-7000-8000-000000000003",
      })
  }
)

it.each(["teams_create", "teams_set_permissions"])(
  "requires authoritative permission completion for %s",
  async (operationId) => {
    for (const receiptStatus of ["applied", undefined]) {
      const permission_changes = receiptStatus
        ? {
            status: receiptStatus,
            applied: [],
            not_applied: [],
            ignored_count: 0,
          }
        : undefined
      vi.stubGlobal(
        "fetch",
        vi.fn(async () =>
          Response.json(
            operationId === "teams_create"
              ? {
                  data: {
                    team_id: "01950000-0000-7000-8000-000000000003",
                    permission_changes,
                  },
                }
              : { message: "ok", permission_changes },
            { status: operationId === "teams_create" ? 201 : 200 }
          )
        )
      )
      const result = await callBackend(
        {
          operationId,
          summary: "Change team permissions",
          method: "POST",
          path: "/workspace/w1/users/teams",
          write: true,
          idempotent: false,
        },
        {
          token: "staff",
          workspaceId: "w1",
          apiUrl: "https://api.test/v1",
          permissions: [],
          sessionId: "s1",
        },
        "a1"
      )
      expect(result.status).toBe(receiptStatus ? "completed" : "unknown")
    }
  }
)

it.each([
  ["queued", "queued"],
  ["accepted", "queued"],
  ["blocked", "partial"],
  ["manual", "partial"],
  ["failed", "partial"],
  ["unknown", "unknown"],
] as const)(
  "keeps invoice numbering distinct from %s delivery",
  async (deliveryStatus, expected) => {
    const delivery = {
      invoice_delivery_id: "01950000-0000-7000-8000-000000000003",
      invoice_id: "01950000-0000-7000-8000-000000000004",
      status: deliveryStatus,
      send_log_id: null,
      pdf_file_id: null,
    }
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => Response.json({ message: "ok", delivery }))
    )
    for (const operationId of ["invoices_send", "invoices_resend"]) {
      const result = await callBackend(
        {
          operationId,
          summary: "Send invoice",
          method: "POST",
          path: "/workspace/w1/invoices/i1/send",
          write: true,
          idempotent: false,
        },
        {
          token: "staff",
          workspaceId: "w1",
          apiUrl: "https://api.test/v1",
          permissions: [],
          sessionId: "s1",
        },
        "a1"
      )
      expect(result.status).toBe(expected)
      expect(result.data).toMatchObject({ delivery })
    }
  }
)

it.each([
  ["accepted", "queued"],
  ["blocked", "failed"],
  ["manual", "partial"],
  ["no_recipients", "failed"],
] as const)(
  "projects %s mail receipts truthfully for direct and enveloped responses",
  async (receiptStatus, expected) => {
    for (const operationId of ["messages_send", "messages_profile_test"]) {
      const delivery = { status: receiptStatus, send_log_id: null }
      vi.stubGlobal(
        "fetch",
        vi.fn(async () =>
          Response.json(
            operationId === "messages_send"
              ? { message: "ok", delivery }
              : {
                  data: {
                    success_connection: receiptStatus === "accepted",
                    delivery,
                  },
                }
          )
        )
      )
      const result = await callBackend(
        {
          operationId,
          summary: "Send test email",
          method: operationId === "messages_send" ? "POST" : "GET",
          path: "/workspace/w1/messages/send",
          write: true,
          idempotent: false,
        },
        {
          token: "staff",
          workspaceId: "w1",
          apiUrl: "https://api.test/v1",
          permissions: [],
          sessionId: "s1",
        },
        "a1"
      )
      expect(result.status).toBe(expected)
      expect(result.data).toMatchObject({ delivery })
      expect(result.message).toBeTruthy()
    }
  }
)
it("adds only authoritative farm roles from this workspace", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) =>
      Response.json(
        url.endsWith("/me/workspaces")
          ? { data: [{ id: "w1" }] }
          : url.endsWith("/farm/me")
            ? {
                data: {
                  memberships: [
                    { workspace_id: "w1", role: "ops" },
                    { workspace_id: "w2", role: "admin" },
                  ],
                },
              }
            : {
                data: {
                  user: {
                    permissions: [
                      { workspace: "w1", permissions: ["order.get"] },
                    ],
                  },
                },
              }
      )
    )
  )
  const result = await authorizeVoice(
    new Request("http://office.test/tools", {
      headers: { Authorization: "Bearer staff" },
    }),
    "w1"
  )
  expect(result.permissions).toContain("farm:ops")
  expect(result.permissions).not.toContain("farm:admin")
})
it("refuses a workspace outside the authenticated membership", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) =>
      Response.json(
        url.endsWith("/me/workspaces")
          ? { data: [{ id: "w1" }] }
          : { data: { user: { permissions: [] } } }
      )
    )
  )
  const request = new Request("http://office.test/api/voice/tools", {
    headers: { Authorization: "Bearer staff-one" },
  })
  await expect(authorizeVoice(request, "w2")).rejects.toMatchObject({
    status: 403,
  })
})
it("preserves each request's bearer and does not forward model-supplied headers", async () => {
  const fetcher = vi
    .fn<(url: string, options: RequestInit) => Promise<Response>>()
    .mockImplementation(async () => Response.json({ data: [] }))
  vi.stubGlobal("fetch", fetcher)
  const action = {
    operationId: "orders_list",
    summary: "List orders",
    method: "GET",
    path: "/workspace/w1/orders",
    write: false,
    idempotent: false,
  }
  await callBackend(
    action,
    {
      token: "staff-one",
      workspaceId: "w1",
      permissions: [],
      apiUrl: "http://backend.test/v1",
    },
    "a"
  )
  await callBackend(
    action,
    {
      token: "staff-two",
      workspaceId: "w1",
      permissions: [],
      apiUrl: "http://backend.test/v1",
    },
    "b"
  )
  expect(fetcher.mock.calls[0]?.[1]).toHaveProperty(
    "headers.Authorization",
    "Bearer staff-one"
  )
  expect(fetcher.mock.calls[1]?.[1]).toHaveProperty(
    "headers.Authorization",
    "Bearer staff-two"
  )
})

it("never treats a postcommit invoice rejection as proof nothing changed", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () =>
      Response.json(
        { message: "Mail refused after numbering" },
        { status: 422 }
      )
    )
  )
  const result = await callBackend(
    {
      operationId: "invoices_send",
      summary: "Send invoice",
      method: "POST",
      path: "/workspace/w1/invoices/i1/send",
      body: {},
      write: true,
      idempotent: false,
    },
    {
      token: "staff",
      workspaceId: "w1",
      apiUrl: "https://api.test/v1",
      permissions: [],
      sessionId: "s1",
    },
    "a1"
  )
  expect(result.status).toBe("unknown")
})

it("forwards durable identity on writes and accepts a documented empty response", async () => {
  const fetcher = vi.fn<
    (url: string, options: RequestInit) => Promise<Response>
  >(async () => new Response(null, { status: 204 }))
  vi.stubGlobal("fetch", fetcher)
  const result = await callBackend(
    {
      operationId: "product_variants_delete",
      summary: "Delete variant",
      method: "DELETE",
      path: "/workspace/w1/products/p/variants/v",
      write: true,
      idempotent: false,
    },
    {
      token: "staff",
      workspaceId: "w1",
      apiUrl: "https://api.test/v1",
      permissions: [],
      sessionId: "s1",
    },
    "a1"
  )
  expect(result.status).toBe("completed")
  expect(fetcher.mock.calls[0]?.[1]).toHaveProperty(
    "headers.Assistant-Action-Id",
    "a1"
  )
  expect(fetcher.mock.calls[0]?.[1]).toHaveProperty(
    "headers.Assistant-Session-Id",
    "s1"
  )
})

it("passes a refused call on in words, with its reason and the refused fields", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () =>
      Response.json(
        {
          message: "Quantity must be greater than zero.",
          errors: { quantity: ["min"], warehouse_id: ["required"] },
        },
        { status: 422 }
      )
    )
  )
  const result = await callBackend(
    {
      operationId: "orders_create",
      summary: "Create order",
      method: "POST",
      path: "/workspace/w1/orders",
      write: true,
      idempotent: false,
    },
    {
      token: "staff",
      workspaceId: "w1",
      apiUrl: "https://api.test/v1",
      permissions: [],
      sessionId: "s1",
    },
    "a1"
  )
  expect(result).toMatchObject({
    status: "unknown",
    reason: "invalid",
    httpStatus: 422,
    message: "Quantity must be greater than zero.",
    fields: ["quantity", "warehouse_id"],
  })
})

it("attaches a picture to a schema-checked read result, built by the server", async () => {
  const summary = {
    from: "2026-11-07T00:00:00+03:00",
    to: "2026-11-08T00:00:00+03:00",
    source: "till",
    currencies: [
      { currency: "TZS", decimals: 0, gross_total: 1250000, refunded_total: 0, orders: 42 },
    ],
  }
  vi.stubGlobal("fetch", vi.fn(async () => Response.json({ data: summary })))
  const result = await callBackend(
    {
      operationId: "orders_sales_summary",
      summary: "Sales in a time range, per currency",
      method: "GET",
      path: "/workspace/w1/sales-summary?from=2026-11-07T00%3A00%3A00%2B03%3A00&to=2026-11-08T00%3A00%3A00%2B03%3A00&source=till",
      write: false,
      idempotent: true,
    },
    {
      token: "staff",
      workspaceId: "w1",
      apiUrl: "https://api.test/v1",
      permissions: [],
      sessionId: "s1",
    },
    "a1"
  )
  expect(result).toMatchObject({
    status: "completed",
    view: { kind: "sales", rows: [{ currency: "TZS", gross: 1250000, orders: 42 }] },
  })
})
