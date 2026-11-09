import type { MockReply } from "../../src/api-mock"

const tzs = (minor: number) => ({ minor, currency: "TZS", decimals: 2, currency_id: "tzs" })
const meta = (total: number) => ({ current_page: 1, from: total ? 1 : null, to: total || null, last_page: 1, per_page: 100, total })

const line = (id: string, kind: string, minor: number, after: number, at: string) => ({
  id, kind, amount: tzs(minor), balance_after: tzs(after), attribution_id: null, payout_id: null, created_at: at,
})

/** A passbook page: a sale credit of TZS 8,000.00, then a payout of TZS 3,000.00. */
export const passbook: MockReply = {
  body: {
    data: [
      line("l2", "payout", -300000, 500000, "2026-11-02T08:00:00Z"),
      line("l1", "sale_credit", 800000, 800000, "2026-11-01T08:00:00Z"),
    ],
    meta: meta(2),
    balances: [tzs(500000)],
  },
}

/** After an approved delivery correction: the farmer owes TZS 1,200.00 back. */
export const corrected: MockReply = {
  body: {
    data: [
      {
        ...line("l3", "supply_correction", -620000, -120000, "2026-11-09T08:00:00Z"),
        correction_id: "cor-1",
        reason: "Scale read 20 kg high",
      },
      line("l2", "payout", -300000, 500000, "2026-11-02T08:00:00Z"),
      line("l1", "sale_credit", 800000, 800000, "2026-11-01T08:00:00Z"),
    ],
    meta: meta(3),
    balances: [tzs(-120000)],
  },
}

export const emptyPassbook: MockReply = { body: { data: [], meta: meta(0), balances: [] } }

export const payout = (id: string, minor: number) => ({
  id, client_ref: `c-${id}`, person_id: "p1", village_id: "v1", amount: tzs(minor), id_type: "nida",
  paid_by: "u1", paid_at: "2026-11-02T08:00:00Z", void: null,
})

export const payouts: MockReply = { body: { data: [payout("po1", 300000)], meta: meta(1) } }
export const noPayouts: MockReply = { body: { data: [], meta: meta(0) } }

/** POST /farm/payouts answers. */
export const paid: MockReply = { status: 201, body: { data: payout("po2", 200000) } }
export const overBalance: MockReply = {
  status: 422,
  body: { message: "The given data was invalid.", errors: { amount: ["The amount must not be greater than the balance of 5000.00."] } },
}
export const fourEyes: MockReply = {
  status: 403,
  body: { message: "you registered this household, so another staff member must pay this farmer" },
}
export const overLimit: MockReply = { status: 403, body: { message: "this payout is above the officer limit: ops or admin must pay it" } }
export const unavailable: MockReply = { status: 503, body: { message: "service unavailable" } }
