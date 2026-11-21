// Hand-typed mirror of docs/integrations/contracts.md §5.2-§5.5 (contracts v1.1).
// Narrow view types (unions, typed snapshot) over the generated models; types.check.ts keeps
// each one assignable to its generated model, so API drift fails tsc.

export type ApprovalStatus =
  "blocked" | "pending" | "approved" | "rejected" | "returned" | "invalidated"

export type ExecutionStatusCode =
  | "queued"
  | "running"
  | "blocked"
  | "awaiting_reconciliation"
  | "cancelled"
  | "succeeded"
  | "succeeded_with_variance"
  | "resolved_manual"
  | "failed"

export type ExecutionPhase =
  "prechecking" | "write_intent" | "write_sent" | "verifying" | "done"

export type ResolveAction =
  | "link_existing"
  | "allow_retry"
  | "close_no_po"
  | "accept_variance"
  | "reject_variance"

export type IncidentKind = string

export type MappingEntity =
  "company" | "vendor" | "product" | "uom" | "currency" | "user" | "location"

/** One parsed `blocked_reasons` entry: `code[:entity[:source_id]]`. */
export type BlockedReasonView = {
  code: string
  entity?: string | null
  source_id?: string | null
}

export type Actor = { user_id: string; name: string }

export type ExecutionView = {
  id: string
  status: ExecutionStatusCode
  phase: ExecutionPhase
  reason: string | null
  resolution: string | null
  error_class: string | null
  error_message: string | null
  target_ref: string | null
  verified: boolean
  updated_at: string
  /** Present on the full Execution (§5.5), not on the summary inside a request. */
  allowed_actions?: (ResolveAction | "retry")[]
}

export type SnapshotRef = {
  source_id: string | null
  target_id: string | null
  label?: string | null
}

export type SnapshotLine = {
  source_line_id: string
  product: SnapshotRef
  uom: SnapshotRef
  quantity: string
  price_unit: string
  subtotal_minor: number
  date_planned: string | null
}

export type Snapshot = {
  v: 1
  source: {
    connection_id: string
    model: string
    id: string
    name: string | null
    state: string
  }
  target: { connection_id: string; record_type: string }
  company: SnapshotRef
  vendor: SnapshotRef
  currency: SnapshotRef & { ref: string; decimals: number }
  location: SnapshotRef | null
  requester: { source_id: string; user_id: string } | null
  lines: SnapshotLine[]
  amount_untaxed_minor: number
  amount_tax_minor: number
  amount_total_minor: number
  tax_note: string
  mapping_ids: Record<string, string>
}

export type ApprovalRequestView = {
  id: string
  workflow_key: string
  pair_id: string
  subject_ref: string
  revision: number
  fingerprint: string
  status: ApprovalStatus
  blocked_reasons: BlockedReasonView[]
  invalidated_reason: string | null
  amount: { minor: number; currency_ref: string; decimals: number }
  required_permission: string | null
  requester: Actor | null
  decision: {
    decision: "approve" | "reject" | "return"
    actor: Actor
    comment: string | null
    created_at: string
  } | null
  can_decide: boolean
  execution: ExecutionView | null
  open_incidents: { id: string; kind: IncidentKind }[]
  /** Detail only. */
  snapshot?: Snapshot
  created_at: string
  updated_at: string
}

export type CapabilityCheck = {
  key: string
  ok: boolean
  error_class:
    | "transient"
    | "ambiguous"
    | "auth"
    | "permission"
    | "not_found"
    | "validation"
    | "conflict"
    | "unknown"
    | null
  detail: string | null
}

export type ConnectionView = {
  id: string
  provider: "odoo" | "netsuite"
  label: string
  status:
    | "draft"
    | "active"
    | "degraded"
    | "unauthorized"
    | "expired"
    | "disconnected"
  status_reason: string | null
  config: Record<string, unknown>
  secrets: Record<string, string | null>
  credential_expires_at: string | null
  last_probe: CapabilityCheck[] | null
  last_probe_at: string | null
  created_at: string
  updated_at: string
}

export type PairView = {
  id: string
  workflow_key: string
  source_connection_id: string
  target_connection_id: string
  status: "draft" | "active" | "paused"
  paused_reason: string | null
  config: Record<string, unknown>
  activated_at: string | null
  last_polled_at: string | null
  created_at: string
  updated_at: string
}

export type MappingView = {
  id: string
  pair_id: string
  entity: MappingEntity
  source_id: string
  target_id: string
  source_label: string | null
  target_label: string | null
  updated_at: string
}

/** WF-03 incident category (contracts §11.3). */
export type IncidentCategory =
  | "transient"
  | "authorization"
  | "data"
  | "conflict"
  | "missing"
  | "unknown_outcome"

/** Incident (contracts §5.5 + §11.7). */
export type IncidentView = {
  id: string
  kind: IncidentKind
  subject_ref: string
  status: "open" | "resolved"
  pair_id: string | null
  approval_request_id: string | null
  execution_id: string | null
  note: string | null
  resolved_by: string | null
  created_at: string
  category: IncidentCategory | null
  diagnosis: {
    sentence: string
    records: { subject_ref: string; target_id: string | null }[]
    last_attempt: {
      attempt_no: number
      outcome: string | null
      http_status: number | null
    } | null
  } | null
  verified_at: string | null
  resolution_kind: "reconciled" | "manual_closed" | null
}

export type IncidentDetailView = IncidentView & {
  expectation: {
    status: "pending" | "met" | "late" | "failed" | "closed"
    due_at: string
    checks: number
    last_checked_at: string | null
    target_kind: string
    target_external_id: string
  } | null
  timeline: {
    kind: string
    actor_kind: string
    created_at: string
    summary: string
  }[]
}

/** Translator for the shared `businessApprovals` messages. Method syntax so next-intl's `t` fits. */
export type Translate = {
  (key: string, values?: Record<string, string | number>): string
  has?(key: string): boolean
}
