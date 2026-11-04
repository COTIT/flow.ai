import type { MockReply } from "../../src/api-mock"

export const VOUCHER = "60000000-0000-4000-8000-000000000001"
const AMOUNT = { minor: 500000, decimals: 2, currency: "TZS", currency_id: "c-tzs" }

/** POST /workspace/w1/farm/vouchers/lookup: a voucher this officer may hand over, or one the API blocks. */
export const lookup = (over: Record<string, unknown> = {}): MockReply => ({
  body: {
    data: {
      found: true,
      voucher_id: VOUCHER,
      status: "issued",
      expired: false,
      amount: AMOUNT,
      issued_at: "2026-09-20T07:00:00Z",
      expires_at: "2026-12-20T07:00:00Z",
      redeemed_at: null,
      survey_title_en: "Irrigation interest",
      survey_title_sw: "Nia ya umwagiliaji",
      household_label: "Kaya ya Mwakalinga",
      head_name: "Neema Mwakalinga",
      respondent_name: "Daudi Mwakalinga",
      needs_ops: false,
      can_redeem: true,
      blocked_reason: null,
      ...over,
    },
  },
})

export const held = lookup({
  can_redeem: false,
  needs_ops: true,
  blocked_reason: "this voucher is held for an audit: ops or admin must redeem it in person",
})

/** Past its expiry: the API refuses the hand-over and says when (farm_vouchers/rules.rs). */
export const expired = lookup({
  expired: true,
  expires_at: "2026-10-21T07:00:00Z",
  can_redeem: false,
  blocked_reason: "this voucher expired on 21 Oct 2026",
})

/** POST /workspace/w1/farm/vouchers/redeem. */
export const redeemed = {
  data: {
    voucher_id: VOUCHER,
    status: "redeemed",
    redeemed_at: "2026-10-29T07:42:00Z",
    redeemed_by: { id: "u1", name: "Salima Officer" },
    amount: AMOUNT,
    replayed: false,
  },
}

/** GET /workspace/w1/farm/vouchers/{voucher}/timeline. */
export const timeline: MockReply = {
  body: {
    data: [
      { occurred_at: "2026-09-20T07:00:00Z", kind: "issued", actor_name: "Daudi Mwakalinga", actor_role: "farmer", detail: {} },
      { occurred_at: "2026-10-29T07:41:00Z", kind: "scanned", actor_name: "Salima Officer", actor_role: "field_officer", detail: {} },
      { occurred_at: "2026-10-29T07:42:00Z", kind: "redeemed", actor_name: "Salima Officer", actor_role: "field_officer", detail: { id_type_seen: "voter" } },
    ],
  },
}
