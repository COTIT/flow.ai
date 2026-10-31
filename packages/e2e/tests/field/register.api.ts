import type { MockReply } from "../../src/api-mock"

export const MAIZE = "40000000-0000-4000-8000-000000000001"
export const PERSON = "50000000-0000-4000-8000-000000000001"

/** GET /workspace/w1/farm/crops: one active crop measured by area, one retired. */
export const crops: MockReply = {
  body: {
    data: [
      { id: MAIZE, code: "MZ", name_en: "Maize", name_sw: "Mahindi", measured_by: "area", is_active: true, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z" },
      { id: "40000000-0000-4000-8000-000000000009", code: "OLD", name_en: "Retired", name_sw: "Zamani", measured_by: "area", is_active: false, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z" },
    ],
    meta: { current_page: 1, from: 1, to: 2, last_page: 1, per_page: 100, total: 2 },
  },
}

/** POST /workspace/w1/farm/registrations: 201 first time, 200 `replayed` on a retry. */
export const registered = (replayed = false): MockReply => ({
  status: replayed ? 200 : 201,
  body: {
    data: {
      person_id: PERSON,
      household_id: "h1",
      farm_id: "f1",
      plot_id: "pl1",
      village_id: "v1",
      crop_cycle_ids: ["c1"],
      harvest_report_ids: [],
      replayed,
    },
  },
})

export const phoneRefused: MockReply = {
  status: 422,
  body: {
    message: "phone number must be a Tanzanian mobile number, for example +255 712 345 678",
    errors: { "person.phone": ["phone number must be a Tanzanian mobile number, for example +255 712 345 678"] },
  },
}

/** POST /workspace/w1/farm/persons/{person}/login. */
export const issuedLogin: MockReply = {
  body: { data: { phone: "+255712345678", temp_password: "K7QXM2PA", kind: "initial" } },
}
