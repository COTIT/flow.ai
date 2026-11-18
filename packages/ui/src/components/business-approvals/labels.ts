import type { BlockedReasonView, Translate } from "./types"

/**
 * Every code in contracts.md §3 and §5.6, by group. The text lives in the shared
 * `businessApprovals` messages (@genesy/i18n); `label` turns a code into it. A code the
 * UI does not know yet reads as the group's `unknown` text, never as the raw code.
 */
export const CODES = {
  approvalStatus: [
    "blocked",
    "pending",
    "approved",
    "rejected",
    "returned",
    "invalidated",
  ],
  executionStatus: [
    "queued",
    "running",
    "blocked",
    "awaiting_reconciliation",
    "cancelled",
    "succeeded",
    "succeeded_with_variance",
    "resolved_manual",
    "failed",
  ],
  phase: ["prechecking", "write_intent", "write_sent", "verifying", "done"],
  blocked: [
    "unmapped",
    "no_policy",
    "no_requester",
    "no_eligible_approver",
    "amount_not_exact",
    "rate_precision",
    "untotalled_lines",
    "downpayment_line",
    "connection_inactive",
  ],
  invalidated: [
    "source_changed",
    "source_cancelled",
    "source_gone",
    "source_ineligible",
  ],
  executionReason: [
    "connection_unauthorized",
    "connection_expired",
    "connection_inactive",
    "permission_denied",
    "validation_rejected",
    "mapping_stale",
    "mapping_changed",
    "approver_revoked",
    "rate_precision",
    "untotalled_lines",
    "downpayment_line",
    "pair_paused",
    "source_changed",
    "attempts_exhausted",
  ],
  resolution: ["linked_with_differences", "close_no_po", "variance_accepted"],
  action: [
    "retry",
    "link_existing",
    "allow_retry",
    "close_no_po",
    "accept_variance",
    "reject_variance",
  ],
  error: [
    "four_eyes_required",
    "approval_already_decided",
    "approval_stale",
    "approval_not_resubmittable",
    "execution_state_changed",
    "retry_needs_grant",
    "retry_already_granted",
    "incident_already_resolved",
    "connection_not_active",
    "source_controls_unmet",
    "link_unavailable",
  ],
  incident: ["duplicate_obligation_risk", "tax_variance"],
  entity: [
    "company",
    "vendor",
    "product",
    "uom",
    "currency",
    "user",
    "location",
  ],
  connectionStatus: [
    "draft",
    "active",
    "degraded",
    "unauthorized",
    "expired",
    "disconnected",
  ],
  connectionReason: [
    "probe_failed",
    "credential_expiring",
    "credential_expired",
    "auth_failed",
    "permission_denied",
  ],
  pairStatus: ["draft", "active", "paused"],
  pausedReason: ["odoo_control_drift", "connection_inactive", "manual"],
  provider: ["odoo", "netsuite"],
} as const

export type CodeGroup = keyof typeof CODES

/** Readable text for `code` in `group`; unknown or empty codes get the group's `unknown` text. */
export function label(
  t: Translate,
  group: CodeGroup,
  code: string | null | undefined
) {
  // Status reasons may carry a suffix (`probe_failed:<capability>`): the code is the head.
  const head = (code ?? "").split(":")[0]!
  return (CODES[group] as readonly string[]).includes(head)
    ? t(`${group}.${head}`)
    : t(`${group}.unknown`)
}

/** A blocked reason in words, e.g. "Vendor 43 has no NetSuite match". */
export function blockedReason(t: Translate, r: BlockedReasonView) {
  if (r.code === "unmapped")
    return t("blocked.unmappedEntity", {
      entity: label(t, "entity", r.entity),
      id: r.source_id ?? "",
    })
  return label(t, "blocked", r.code)
}

/** The readable sentence for an API error: by `code`, then status; never the raw code. */
export function errorText(t: Translate, error: unknown) {
  const response = (
    error as {
      response?: { status?: number; data?: { code?: string; message?: string } }
    } | null
  )?.response
  const code = response?.data?.code
  if (code) return label(t, "error", code)
  // A field error (422) is the API's own readable sentence.
  if (response?.status === 422 && response.data?.message)
    return response.data.message
  if (response?.status === 403) return t("error.forbidden")
  if (response?.status === 404) return t("error.notFound")
  return t("error.unknown")
}
