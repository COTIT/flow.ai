import type { FakeApi } from "../office/fake-api"

/**
 * WF-01 fake API, built only from docs/integrations/contracts.md §5.7 (Sample data).
 * Changes from §5.7, all forced by the sample's placeholders:
 * - "9f2c…64hex"-style fingerprints are padded to 64 hex characters;
 * - §5.7 shows r1 only as a list row and r2 (blocked) in full; r1's detail reuses r2's
 *   snapshot with the ids of order 17 and every target_id filled (§5.4: null only when blocked);
 * - r5 (behind execution e2) and r6 (tax variance) are r1 with the §5.7 execution/incident
 *   shapes attached.
 * shortcut: mocked phase only; the stack phase runs the same flows against the real API.
 */

export const hex64 = (head: string) => head.padEnd(64, "0")

export const meta1 = {
  current_page: 1,
  from: 1,
  last_page: 1,
  per_page: 50,
  to: 1,
  total: 1,
}
const list = <T>(rows: T[]) => ({
  data: rows,
  meta: {
    ...meta1,
    from: rows.length ? 1 : null,
    to: rows.length || null,
    total: rows.length,
  },
})

export const errors = {
  forbidden: { message: "This action is unauthorized." },
  notFound: { message: "The requested resource could not be found." },
  stale: {
    message:
      "This request changed after you opened it. Reload to see the new version.",
    code: "approval_stale",
  },
  decided: {
    message: "This request was already decided.",
    code: "approval_already_decided",
  },
  fourEyes: {
    message: "You cannot decide a request you raised.",
    code: "four_eyes_required",
  },
  commentRequired: {
    message: "The comment field is required.",
    errors: { comment: ["The comment field is required."] },
  },
  stateChanged: {
    message: "This run changed state. Reload and try again.",
    code: "execution_state_changed",
  },
  linkUnavailable: {
    message: "Linking an existing order is not available yet.",
    code: "link_unavailable",
  },
  connection422: {
    message: "The label field is required. (and 1 more error)",
    errors: {
      "config.base_url": ["The base url must be a valid URL."],
      label: ["The label field is required."],
    },
  },
  controlsUnmet: {
    message:
      "Odoo purchase approval settings do not meet the requirement for: Acme BV.",
    code: "source_controls_unmet",
  },
}

// --- Approvals (§5.7 "Approvals") ---------------------------------------------------------

export const r1Row = {
  id: "r1",
  workflow_key: "wf01_purchase",
  pair_id: "p1",
  subject_ref: "odoo:c1:purchase.order:17",
  revision: 1,
  fingerprint: hex64("9f2c"),
  status: "pending",
  blocked_reasons: [] as {
    code: string
    entity?: string
    source_id?: string
  }[],
  invalidated_reason: null as string | null,
  amount: { minor: 125000, currency_ref: "EUR", decimals: 2 },
  required_permission: "integration.approve" as string | null,
  requester: { user_id: "u1", name: "Ana Buyer" },
  decision: null as unknown,
  can_decide: true,
  execution: null as unknown,
  open_incidents: [] as { id: string; kind: string }[],
  created_at: "2026-11-17T11:00:00Z",
  updated_at: "2026-11-17T11:00:00Z",
}

export const r2 = {
  id: "r2",
  workflow_key: "wf01_purchase",
  pair_id: "p1",
  subject_ref: "odoo:c1:purchase.order:18",
  revision: 1,
  fingerprint: hex64("1a7e"),
  status: "blocked",
  blocked_reasons: [
    { code: "unmapped", entity: "vendor", source_id: "43" },
    { code: "unmapped", entity: "product", source_id: "9" },
  ],
  invalidated_reason: null,
  amount: { minor: 30000, currency_ref: "EUR", decimals: 2 },
  required_permission: null,
  requester: { user_id: "u1", name: "Ana Buyer" },
  decision: null,
  can_decide: false,
  execution: null,
  open_incidents: [],
  snapshot: {
    v: 1,
    source: {
      connection_id: "c1",
      model: "purchase.order",
      id: "18",
      name: "P00018",
      state: "draft",
    },
    target: { connection_id: "c3", record_type: "purchaseOrder" },
    company: { source_id: "1", target_id: "3", label: "Acme BV" },
    vendor: {
      source_id: "43",
      target_id: null as string | null,
      label: "Deco Addict",
    },
    currency: { source_id: "1", target_id: "1", ref: "EUR", decimals: 2 },
    location: null,
    requester: { source_id: "2", user_id: "u1" },
    lines: [
      {
        source_line_id: "55",
        product: {
          source_id: "9",
          target_id: null as string | null,
          label: "Desk",
        },
        uom: { source_id: "1", target_id: "1", label: "Units" },
        quantity: "2.00000000",
        price_unit: "150.00000000",
        subtotal_minor: 30000,
        date_planned: null,
      },
    ],
    amount_untaxed_minor: 30000,
    amount_tax_minor: 6300,
    amount_total_minor: 36300,
    tax_note: "odoo_estimate_target_calculates",
    mapping_ids: {
      "company:1": "m7",
      "currency:1": "m8",
      "uom:1": "m9",
      "user:2": "m10",
    },
  },
  created_at: "2026-11-17T11:00:00Z",
  updated_at: "2026-11-17T11:00:00Z",
}

/** r1 in full: r2's snapshot shape with order 17's ids and every target filled. */
export const r1 = {
  ...r1Row,
  snapshot: {
    ...r2.snapshot,
    source: { ...r2.snapshot.source, id: "17", name: "P00017" },
    vendor: { source_id: "42", target_id: "1187", label: "Azure Interior" },
    lines: [
      {
        ...r2.snapshot.lines[0]!,
        product: { source_id: "9", target_id: "501", label: "Desk" },
        quantity: "5.00000000",
        price_unit: "250.00000000",
        subtotal_minor: 125000,
      },
    ],
    amount_untaxed_minor: 125000,
    amount_tax_minor: 26250,
    amount_total_minor: 151250,
  },
}

export const approvedExecution = {
  id: "e1",
  status: "queued",
  phase: "prechecking",
  reason: null,
  resolution: null,
  error_class: null,
  error_message: null,
  target_ref: null,
  verified: false,
  updated_at: "2026-11-17T11:05:00Z",
}
export const approveDecision = {
  decision: "approve",
  actor: { user_id: "u2", name: "Ben Approver" },
  comment: null,
  created_at: "2026-11-17T11:05:00Z",
}

// --- Workflows (§5.7 "Workflows") ---------------------------------------------------------

export const e2 = {
  id: "e2",
  approval_request_id: "r5",
  workflow_key: "wf01_purchase",
  correlation_id: "vz-0f1e2d3c4b5a69788796a5b4c3d2e1f0",
  status: "awaiting_reconciliation",
  phase: "write_sent",
  reason: null,
  resolution: null as string | null,
  error_class: "ambiguous" as string | null,
  error_message:
    "NetSuite did not answer in time. Flow is checking whether the order was created." as
      string | null,
  target_ref: null as string | null,
  verified: false,
  remote_attempts: 1,
  manual_retry_grants: 0,
  allowed_actions: ["link_existing", "close_no_po"],
  created_at: "2026-11-17T11:10:00Z",
  updated_at: "2026-11-17T11:12:00Z",
}
const summary = (e: typeof e2) => {
  const {
    approval_request_id: _a,
    workflow_key: _w,
    correlation_id: _c,
    remote_attempts: _r,
    manual_retry_grants: _m,
    allowed_actions: _x,
    created_at: _d,
    ...s
  } = e
  void [_a, _w, _c, _r, _m, _x, _d]
  return s
}
export const e2Linked = {
  ...e2,
  status: "resolved_manual",
  resolution: "linked_with_differences",
  target_ref: "netsuite:c3:purchaseOrder:8812",
  verified: false,
  allowed_actions: [],
}
export const r5 = {
  ...r1,
  id: "r5",
  subject_ref: "odoo:c1:purchase.order:21",
  fingerprint: hex64("5e5e"),
  status: "approved",
  can_decide: false,
  decision: approveDecision,
  execution: summary(e2),
  snapshot: {
    ...r1.snapshot,
    source: { ...r1.snapshot.source, id: "21", name: "P00021" },
  },
}

/** A created order whose tax differs from Odoo's estimate (status per §3.2, shapes per §5.7). */
export const e6 = {
  ...e2,
  id: "e6",
  approval_request_id: "r6",
  status: "succeeded_with_variance",
  phase: "done",
  error_class: null,
  error_message: null,
  target_ref: "netsuite:c3:purchaseOrder:8813",
  verified: true,
  allowed_actions: ["accept_variance", "reject_variance"],
}
export const r6 = {
  ...r5,
  id: "r6",
  subject_ref: "odoo:c1:purchase.order:22",
  fingerprint: hex64("6e6e"),
  execution: summary(e6),
  open_incidents: [{ id: "i2", kind: "tax_variance" }],
  snapshot: {
    ...r1.snapshot,
    source: { ...r1.snapshot.source, id: "22", name: "P00022" },
  },
}

const row = <T extends { snapshot?: unknown }>(r: T) => {
  const { snapshot: _s, ...rest } = r
  void _s
  return rest
}

/**
 * Approval requests and their executions, statefully: approve moves r1 to approved with a
 * queued execution (§5.7 approve → 200); resolve moves e2 to resolved_manual. Every decide
 * and resolve call is recorded on `api.calls`.
 */
export function approvalsApi(
  api: FakeApi,
  requests: { id: string }[] = [r1, r2]
) {
  const byId = new Map(
    requests.map((r) => [r.id, structuredClone(r) as Record<string, unknown>])
  )
  const executions = new Map<string, Record<string, unknown>>([
    ["e2", structuredClone(e2)],
    ["e6", structuredClone(e6)],
  ])
  api
    .on("GET", "/workspace/:ws/approvals/requests", ({ query }) =>
      list(
        [...byId.values()]
          .filter((r) => query.get("scope") !== "decidable" || r.can_decide)
          .map(row)
      )
    )
    .on("GET", "/workspace/:ws/approvals/requests/:id", ({ params }) => {
      const r = byId.get(params[1]!)
      return r ? { data: r } : { status: 404, body: errors.notFound }
    })
    .on(
      "POST",
      "/workspace/:ws/approvals/requests/:id/approve",
      ({ params }) => {
        const r = byId.get(params[1]!)!
        Object.assign(r, {
          status: "approved",
          can_decide: false,
          decision: approveDecision,
          execution: approvedExecution,
        })
        executions.set("e1", {
          ...structuredClone(e2),
          ...approvedExecution,
          approval_request_id: r.id,
          allowed_actions: [],
        })
        return { data: r }
      }
    )
    .on("GET", "/workspace/:ws/workflows/executions/:id", ({ params }) => {
      const e = executions.get(params[1]!)
      return e
        ? { data: { ...e, attempts: [], events: [], resolutions: [] } }
        : { status: 404, body: errors.notFound }
    })
    .on(
      "POST",
      "/workspace/:ws/workflows/executions/:id/resolve",
      ({ params, body }) => {
        const action = (body as { action: string }).action
        const id = params[1]!
        const next =
          action === "link_existing"
            ? e2Linked
            : action === "accept_variance"
              ? {
                  ...e6,
                  status: "resolved_manual",
                  resolution: "variance_accepted",
                  verified: false,
                  allowed_actions: [],
                }
              : executions.get(id)!
        executions.set(id, structuredClone(next) as Record<string, unknown>)
        for (const r of byId.values())
          if ((r.execution as { id?: string } | null)?.id === id)
            r.execution = summary(next as typeof e2)
        return { data: next }
      }
    )
  return api
}

// --- Integrations (§5.7 "Connections", "Pairs and mappings") ------------------------------

export const c1 = {
  id: "c1",
  provider: "odoo",
  label: "Odoo main",
  status: "active",
  status_reason: null,
  config: { base_url: "https://acme.odoo.com", database: null },
  secrets: { api_key: "********" },
  credential_expires_at: "2027-02-01T00:00:00Z",
  last_probe: [
    { key: "purchase_request.read", ok: true, error_class: null, detail: null },
  ],
  last_probe_at: "2026-11-17T10:00:00Z",
  created_at: "2026-11-17T09:00:00Z",
  updated_at: "2026-11-17T10:00:00Z",
}
export const c3 = {
  id: "c3",
  provider: "netsuite",
  label: "NS prod",
  status: "unauthorized",
  status_reason: "auth_failed",
  config: {
    rest_domain: "1234567.suitetalk.api.netsuite.com",
    environment: "production",
    client_id: "abc",
    certificate_id: "kid1",
    alg: "ES256",
  },
  secrets: { private_key: "********" },
  credential_expires_at: "2028-11-01T00:00:00Z",
  last_probe: [
    {
      key: "auth.token",
      ok: false,
      error_class: "auth",
      detail:
        "The token request was refused. Check the certificate and client id.",
    },
  ],
  last_probe_at: "2026-11-17T10:05:00Z",
  created_at: "2026-11-17T09:00:00Z",
  updated_at: "2026-11-17T10:05:00Z",
}
export const p1 = {
  id: "p1",
  workflow_key: "wf01_purchase",
  source_connection_id: "c1",
  target_connection_id: "c3",
  status: "paused",
  paused_reason: "odoo_control_drift",
  config: {
    poll_minutes: 5,
    backfill_days: 0,
    full_scan_minutes: 60,
    succeeded_watch_days: 30,
    tax_tolerance_minor: 0,
    default_locations: { "1": "7" },
  },
  activated_at: "2026-11-17T10:10:00Z",
  last_polled_at: "2026-11-17T11:00:00Z",
  created_at: "2026-11-17T10:00:00Z",
  updated_at: "2026-11-17T11:00:00Z",
}
export const m1 = {
  id: "m1",
  pair_id: "p1",
  entity: "vendor",
  source_id: "42",
  target_id: "1187",
  source_label: "Azure Interior",
  target_label: "Azure Interior Ltd",
  updated_at: "2026-11-17T10:20:00Z",
}

/** Connections, pairs and mappings, statefully. A new connection's secret is never echoed back. */
export function integrationsApi(api: FakeApi) {
  const connections = [structuredClone(c1), structuredClone(c3)] as Record<
    string,
    unknown
  >[]
  const mappings = [structuredClone(m1)] as Record<string, unknown>[]
  api
    .on("GET", "/workspace/:ws/integrations/connections", () =>
      list(connections)
    )
    .on("POST", "/workspace/:ws/integrations/connections", ({ body }) => {
      const b = body as {
        provider: string
        label: string
        config: object
        secrets?: Record<string, string>
      }
      connections.push({
        ...structuredClone(c1),
        id: "c2",
        provider: b.provider,
        label: b.label,
        status: "draft",
        config: b.config,
        // The API stores the secret and only ever answers the mask.
        secrets: Object.fromEntries(
          Object.keys(b.secrets ?? {}).map((k) => [k, "********"])
        ),
        last_probe: null,
        last_probe_at: null,
      })
      return { status: 201, body: { data: { connection_id: "c2" } } }
    })
    .on("POST", "/workspace/:ws/integrations/connections/:id", () => ({
      message: "ok",
    }))
    .on(
      "POST",
      "/workspace/:ws/integrations/connections/:id/test",
      ({ params }) => ({
        data:
          params[1] === "c3" ? c3 : connections.find((c) => c.id === params[1]),
      })
    )
    .on("GET", "/workspace/:ws/integrations/pairs", () => list([p1]))
    .on(
      "GET",
      "/workspace/:ws/integrations/pairs/:pair/mappings",
      ({ query }) =>
        list(
          mappings.filter(
            (m) => !query.get("entity") || m.entity === query.get("entity")
          )
        )
    )
    .on(
      "POST",
      "/workspace/:ws/integrations/pairs/:pair/mappings",
      ({ body }) => {
        mappings.push({
          ...(body as object),
          id: `m${mappings.length + 1}`,
          pair_id: "p1",
          source_label: null,
          target_label: null,
          updated_at: "2026-11-17T12:00:00Z",
        })
        return {
          status: 201,
          body: { data: { mapping_id: `m${mappings.length}` } },
        }
      }
    )
  return api
}
