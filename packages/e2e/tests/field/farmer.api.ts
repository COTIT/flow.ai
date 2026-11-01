import type { MockReply } from "../../src/api-mock"
import { me, role } from "./api"

export const SURVEY = "f1000000-0000-4000-8000-000000000001"
export const BLOCKED = "f1000000-0000-4000-8000-000000000002"
export const VOUCHER = "51000000-0000-4000-8000-000000000001"
export const CODE = "K7QXM2PA9D"
export const BLOCKED_REASON = "your household has not been verified yet"
const REWARD = { minor: 500000, decimals: 2, currency: "TZS", currency_id: "c-tzs" }

/** A farmer signs in by phone; the session has no email. */
export const farmerMe = me({ name: "Neema Mwakalinga", email: null, phone: "0712345678", memberships: [role("farmer")] })

const page = (data: unknown[]): MockReply => ({
  body: { data, meta: { current_page: 1, from: data.length ? 1 : null, to: data.length || null, last_page: 1, per_page: 100, total: data.length } },
})

const provenance = {
  source: "field_verified",
  verification: "verified",
  confidence: "high",
  captured_at: "2026-09-01T08:00:00Z",
  captured_by: null,
  evidence_ref: null,
  verified_at: null,
  verified_by: null,
}
const stamps = { created_at: "2026-09-01T08:00:00Z", updated_at: "2026-09-01T08:00:00Z", village_id: "v1" }

/** The farmer's own records, as the person holdings read returns them for p1. */
export const records: Record<string, MockReply> = {
  "GET /workspace/w1/farm/persons/p1/holdings": {
    body: {
      data: {
        households: [
          {
            id: "h1", label: "Mwakalinga household", members: [{ person_id: "p1", is_head: true }], provenance, ...stamps,
            people: [{ person_id: "p1", given_name: "Neema", family_name: "Mwakalinga", is_head: true }],
          },
        ],
        farms: [
          {
            id: "f1", label: "Riverside farm", household_id: "h1", managers: [{ person_id: "p1", is_primary: true }], latitude: null, longitude: null, provenance, ...stamps,
            plots: [
              {
                id: "pl1", farm_id: "f1", label: "Riverside", area_ha: "1.8000", latitude: null, longitude: null, provenance, ...stamps,
                cycles: [
                  {
                    id: "c1", plot_id: "pl1", crop_id: "cr-mz", status: "growing", season_label: null, planted_on: null, area_ha: "1.6000", tree_count: null, unit_count: null, harvest_start: "2026-09-01", harvest_end: "2026-09-30", provenance, ...stamps,
                    crop: { id: "cr-mz", code: "MZ", name_en: "Maize", name_sw: "Mahindi", measured_by: "area" },
                    harvests: [
                      { id: "hr1", crop_cycle_id: "c1", kind: "expected", is_current: true, quantity_kg: "4100.00", reported_for: null, provenance, ...stamps },
                    ],
                  },
                ],
              },
            ],
          },
        ],
      },
    },
  },
}

const survey = (id: string, title_en: string, questions: unknown[]) => ({
  body: {
    data: {
      id, title_en, title_sw: null, description_en: null, description_sw: null, reward: REWARD,
      closes_at: null, status: "live", village_id: null, max_households: null, audit_percent: 15,
      responses: 0, published_at: "2026-09-01T00:00:00Z", published_by: null, closed_at: null,
      created_at: "2026-09-01T00:00:00Z", updated_at: "2026-09-01T00:00:00Z", questions,
    },
  },
})

/** One question of each kind the farmer answers here. */
export const surveys: Record<string, MockReply> = {
  [`GET /workspace/w1/farm/surveys/${SURVEY}`]: survey(SURVEY, "Maize storage", [
    { id: "q-yn", position: 1, kind: "yes_no", prompt_en: "Do you store maize in bags?", prompt_sw: null, required: true, options: [] },
    {
      id: "q-sc", position: 2, kind: "single_choice", prompt_en: "Where do you store maize?", prompt_sw: null, required: true,
      options: [{ value: "house", label_en: "In the house", label_sw: null }, { value: "crib", label_en: "In a crib", label_sw: null }],
    },
    { id: "q-num", position: 3, kind: "number", prompt_en: "How many bags did you store?", prompt_sw: null, required: true, options: [] },
  ]),
  [`GET /workspace/w1/farm/surveys/${BLOCKED}`]: survey(BLOCKED, "Household energy use", []),
}

const row = (survey_id: string, over: Record<string, unknown> = {}) => ({
  survey_id, eligible: false, reason: null, response_id: null, voucher_id: null, ...over,
})

export const eligibilityBefore = page([row(SURVEY, { eligible: true }), row(BLOCKED, { reason: BLOCKED_REASON })])
export const eligibilityAfter = page([
  row(SURVEY, { response_id: "r1", voucher_id: VOUCHER }),
  row(BLOCKED, { reason: BLOCKED_REASON }),
])

export const noVouchers = page([])
const myVoucher = {
  id: VOUCHER, survey_id: SURVEY, survey_title_en: "Maize storage", survey_title_sw: null, code: CODE,
  amount: REWARD, status: "issued", expired: false, issued_at: "2026-10-30T08:00:00Z",
  expires_at: "2026-11-29T08:00:00Z", redeemed_at: null, void_reason: null,
}
export const myVouchers = page([myVoucher])

/** The same voucher after ops voided it: vouchers/mine carries their reason. */
export const voidedVouchers = page([{ ...myVoucher, status: "void", void_reason: "Duplicate household" }])

/** POST .../responses: 201 the first time, 200 with replayed:true for the same client_ref. */
export const submitted = (replayed: boolean) => ({
  data: { response_id: "r1", voucher_id: VOUCHER, code: CODE, amount: REWARD, expires_at: "2026-11-29T08:00:00Z", replayed },
})

export const timeline: MockReply = {
  body: {
    data: [
      { occurred_at: "2026-10-30T08:00:00Z", kind: "answered", actor_name: "Neema Mwakalinga", actor_role: "farmer", detail: {} },
      { occurred_at: "2026-10-30T08:00:01Z", kind: "issued", actor_name: "Neema Mwakalinga", actor_role: "farmer", detail: {} },
    ],
  },
}

/** What the farmer surface reads once /farm is on the API: the home page and the Surveys badge. */
export const farmerHome: Record<string, MockReply> = {
  ...records,
  "GET /workspace/w1/farm/surveys/eligibility": eligibilityBefore,
}
