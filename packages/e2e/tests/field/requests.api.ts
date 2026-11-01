import type { MockReply } from "../../src/api-mock"
import { MILL } from "./equipment.api"

export const REQUEST = "61000000-0000-4000-8000-000000000001"

const meta = (total: number) => ({ current_page: 1, from: total ? 1 : null, to: total || null, last_page: 1, per_page: 100, total })

/** A PUE request as the API returns it (estimate stored by the API's database). */
export const pueRequest = (over: Record<string, unknown> = {}) => ({
  id: REQUEST,
  village_id: "v1",
  person: { id: "p1", name: "Neema Mwakalinga" },
  farm_id: null,
  equipment: { id: MILL, code: "MILL-500", name_en: "Maize mill", name_sw: "Mashine ya kusaga" },
  acquisition: "buy",
  quantity: 1,
  hours_per_day: "6.00",
  days_per_week: "5.0",
  purpose: "Milling for the co-op",
  status: "submitted",
  submitted_at: "2026-10-01T08:00:00Z",
  decided_at: null,
  decided_by: null,
  decision_note: null,
  estimate: {
    rated_power_kw: "15.000", quantity: 1, hours_per_day: "6.00", days_per_week: "5.0",
    est_power_kw: "15.000", est_kwh_per_day: "90.000", est_kwh_per_week: "450.000",
    method: "level1_rated_x_hours", source: "model_estimated", confidence: null, computed_at: "2026-10-01T08:00:00Z",
  },
  provenance: {
    source: "farmer_reported", verification: "unverified", confidence: null, evidence_ref: null,
    captured_at: "2026-10-01T08:00:00Z", captured_by: { id: "u1", name: "Neema Mwakalinga" },
    verified_at: null, verified_by: null,
  },
  created_at: "2026-10-01T08:00:00Z",
  updated_at: "2026-10-01T08:00:00Z",
  ...over,
})

/** GET .../farm/pue-requests */
export const requestList = (...rows: unknown[]): MockReply => ({ body: { data: rows, meta: meta(rows.length) } })
export const one = (data: unknown, status = 200): MockReply => ({ status, body: { data } })

/** GET .../villages/{v}/energy */
export const energy: MockReply = {
  body: {
    data: {
      village_id: "v1", capacity_kw: "500.00", capacity_basis: "planned", simultaneity_factor: "0.600",
      prospective_kw_raw: "15.000", approved_kw_raw: "0.000", prospective_peak_kw: "9.000", approved_peak_kw: "0.000",
      headroom_kw: "500.000", approved_kwh_per_week: "0.000", approved_assets: 0,
    },
  },
}

/** A review move by someone who is not ops or admin (the API's words). */
export const reviewRefused: MockReply = { status: 403, body: { message: "only ops or admin may review a request" } }
