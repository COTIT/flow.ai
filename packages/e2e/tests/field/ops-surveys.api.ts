import type { MockReply } from "../../src/api-mock"

export const LIVE = "30000000-0000-4000-8000-000000000001"
export const DRAFT = "30000000-0000-4000-8000-000000000002"
export const VOUCHER = "40000000-0000-4000-8000-000000000001"

const TZS = (minor: number) => ({ minor, decimals: 2, currency: "TZS", currency_id: "c-tzs" })
const meta = (total: number) => ({ current_page: 1, from: 1, to: total, last_page: 1, per_page: 100, total })

export const question = {
  id: "q1",
  position: 1,
  kind: "single_choice",
  prompt_en: "Where do you store your harvest?",
  prompt_sw: null,
  required: true,
  options: [
    { value: "crib", label_en: "In a crib", label_sw: null },
    { value: "house", label_en: "In the house", label_sw: null },
  ],
}

const survey = (over: Record<string, unknown>) => ({
  village_id: null,
  title_sw: null,
  description_en: null,
  description_sw: null,
  reward: TZS(500000),
  max_households: null,
  audit_percent: 15,
  closes_at: null,
  closed_at: null,
  published_at: null,
  published_by: null,
  created_by: { id: "u1", name: "Amina Admin" },
  summary: null,
  responses: 0,
  questions: [],
  created_at: "2026-09-19T08:00:00Z",
  updated_at: "2026-09-19T08:00:00Z",
  ...over,
})

export const liveSurvey = survey({
  id: LIVE,
  title_en: "Harvest intentions",
  status: "live",
  responses: 3,
  published_at: "2026-09-20T08:00:00Z",
  published_by: { id: "u1", name: "Amina Admin" },
  // The list's voucher sums, for staff (null for farmers).
  summary: {
    issued: { count: 3, minor: 1500000 },
    redeemed: { count: 1, minor: 500000 },
    outstanding: { count: 1, minor: 500000 },
    expired_count: 0,
    void_count: 1,
  },
  questions: [question],
})

export const draftSurvey = (questions: unknown[] = []) =>
  survey({ id: DRAFT, title_en: "Water use", status: "draft", questions })

export const one = (body: unknown): MockReply => ({ body: { data: body } })

/** GET /workspace/w1/farm/surveys */
export const list: MockReply = { body: { data: [liveSurvey, draftSurvey()], meta: meta(2) } }

/** GET /workspace/w1/currencies: the workspace's TZS is the default. */
export const currencies: MockReply = {
  body: {
    data: [
      { id: "c-eur", name: "Euro", asset_type: "eur", symbol_prefix: "€", symbol_suffix: null, decimals: 2, default_currency: false, active: true, meta: null, created_at: "2026-09-01T00:00:00Z", last_updated_at: "2026-09-01T00:00:00Z" },
      { id: "c-tzs", name: "Tanzanian shilling", asset_type: "tzs", symbol_prefix: null, symbol_suffix: "TZS", decimals: 2, default_currency: true, active: true, meta: null, created_at: "2026-09-01T00:00:00Z", last_updated_at: "2026-09-01T00:00:00Z" },
    ],
    meta: meta(2),
  },
}

/** GET .../surveys/{LIVE}/results: every count and cash sum is the API's. */
export const results: MockReply = {
  body: {
    data: {
      currency: "TZS",
      responses: 3,
      issued: { count: 3, minor: 1500000 },
      redeemed: { count: 1, minor: 500000 },
      outstanding: { count: 1, minor: 500000 },
      expired_count: 0,
      void_count: 1,
      tally: [{ question_id: "q1", position: 1, option_value: "crib", answers: 2 }],
      numbers: [],
    },
  },
}

/** GET .../surveys/{LIVE}/vouchers: no codes. */
export const vouchers: MockReply = {
  body: {
    data: [
      {
        voucher_id: VOUCHER,
        household_label: "Mwakalinga household",
        respondent_name: "Neema Mwakalinga",
        submitted_at: "2026-09-20T07:30:00Z",
        issued_at: "2026-09-20T07:30:00Z",
        expires_at: "2026-10-20T07:30:00Z",
        status: "issued",
        expired: false,
        amount: TZS(500000),
        audit_required: false,
        redeemed_at: null,
        redeemed_by_name: null,
        id_type_seen: null,
        void_reason: null,
      },
    ],
    meta: meta(1),
  },
}

export const timeline: MockReply = {
  body: {
    data: [
      { occurred_at: "2026-09-20T07:30:00Z", kind: "issued", actor_name: "Neema Mwakalinga", actor_role: "farmer", detail: {} },
    ],
  },
}

export const liveRefusal: MockReply = { status: 422, body: { message: "a live survey cannot be edited" } }
