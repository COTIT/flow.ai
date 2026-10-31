import type { MockReply } from "../../src/api-mock"

/** GET /workspace/w1/farm/redemptions/totals: per officer per day, summed by the API in minor units. */
export const totals: MockReply = {
  body: {
    data: [
      { day: "2026-09-29", redeemed_by: { id: "u3", name: "Asha Officer" }, vouchers: 2, minor: 1000000, decimals: 2, currency: "TZS" },
      { day: "2026-09-28", redeemed_by: { id: "u4", name: "Juma Ops" }, vouchers: 1, minor: 500000, decimals: 2, currency: "TZS" },
    ],
  },
}

/** GET /workspace/w1/farm/redemptions: one page of the log. */
export const log: MockReply = {
  body: {
    data: [
      {
        voucher_id: "v1",
        redeemed_at: "2026-09-29T06:15:00Z",
        redeemed_by: { id: "u3", name: "Asha Officer" },
        village_name: "Ilundo",
        household_label: "Mwakalinga household",
        survey_title_en: "Harvest intentions",
        survey_title_sw: "Nia ya mavuno",
        amount: { minor: 500000, decimals: 2, currency: "TZS", currency_id: "c-tzs" },
        id_type_seen: "nida",
        audit_required: false,
      },
    ],
    meta: { current_page: 1, from: 1, to: 1, last_page: 1, per_page: 100, total: 1 },
  },
}

export const notOps: MockReply = {
  status: 403,
  body: { message: "only ops or admin may read redemptions" },
}
