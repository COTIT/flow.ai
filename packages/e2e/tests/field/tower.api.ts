import type { MockReply } from "../../src/api-mock"

/** A uuid: the Tower's search validation drops a village id that is not one. */
export const VILLAGE = "20000000-0000-4000-8000-000000000001"
export const W = "/workspace/w1/farm"
export const V = `${W}/villages/${VILLAGE}`

const meta = (total: number, perPage = 100) => ({
  current_page: 1, from: total ? 1 : null, to: total ? Math.min(total, perPage) : null, last_page: 1, per_page: perPage, total,
})
const list = (data: unknown[]): MockReply => ({ body: { data, meta: meta(data.length) } })
const prov = (verification: string) => ({
  source: "field_verified", verification, confidence: null, evidence_ref: null,
  captured_at: "2026-09-01T08:00:00Z", captured_by: null, verified_at: null, verified_by: null,
})
const stamps = { created_at: "2026-09-01T08:00:00Z", updated_at: "2026-09-01T08:00:00Z" }

export const villages: MockReply = {
  body: {
    data: [{ id: VILLAGE, code: "ILUNDO", name: "Ilundo", latitude: null, longitude: null, primary_language: "sw" }],
    meta: meta(1),
  },
}

export const crops = list([
  { id: "c1", code: "MAIZE", name_en: "Maize", name_sw: "Mahindi", measured_by: "area", is_active: true },
])

export const production: MockReply = {
  body: {
    data: [{
      crop_id: "c1", window_month: "2026-09-01", cycle_count: 2, plot_count: 1, cycle_area_ha: "1.6000",
      tree_count: null, unit_count: null, expected_kg: "3200.00", actual_kg: null, verified_cycles: 1,
    }],
  },
}

export const energy: MockReply = {
  body: {
    data: {
      village_id: VILLAGE, capacity_kw: "500.00", capacity_basis: "planned", simultaneity_factor: "0.600",
      prospective_kw_raw: "15.000", approved_kw_raw: "18.000", prospective_peak_kw: "9.000", approved_peak_kw: "10.800",
      headroom_kw: "489.200", approved_kwh_per_week: "828.000", approved_assets: 2,
    },
  },
}

const tzs = (minor: number) => ({ minor, currency: "TZS", decimals: 2, currency_id: "c-tzs" })
export const pipeline: MockReply = {
  body: {
    data: [
      { status: "approved", request_count: 2, indicative_value: tzs(3860000000), indicative_rent_per_day: null },
      { status: "submitted", request_count: 1, indicative_value: null, indicative_rent_per_day: tzs(15000000) },
    ],
  },
}

export const matches: MockReply = {
  body: {
    data: [{
      buyer_demand_id: "d1", buyer: { id: "b1", name: "Iringa Grain Traders" }, buyer_name: "Iringa Grain Traders",
      crop_id: "c1", demand_kg: "9000.00", window_start: "2026-09-01", window_end: "2026-10-31",
      village: { id: VILLAGE, name: "Ilundo" }, available_kg: "5600.00", committed_kg: "0.00",
      coverable_kg: "5600.00", coverage_pct: "62.2", opportunity: { id: "o1", status: "shared" },
    }],
  },
}

export const quality: MockReply = {
  body: { data: { village_id: VILLAGE, persons: 6, persons_verified: 3, farms: 4, farms_with_gps: 3, cycles: 7, cycles_with_estimate: 5 } },
}

export const persons = list([
  { id: "p1", given_name: "Neema", family_name: "Mushi", phone: null, village_id: VILLAGE, provenance: prov("verified"), ...stamps },
  { id: "p2", given_name: "Joseph", family_name: "Kalinga", phone: null, village_id: VILLAGE, provenance: prov("unverified"), ...stamps },
])

export const cycles = list([{
  id: "cy1", crop_id: "c1", crop: { name_en: "Maize", name_sw: "Mahindi" }, plot_id: "pl1", village_id: VILLAGE,
  season_label: "2026 long rains", area_ha: "1.6000", tree_count: null, unit_count: null, planted_on: null,
  harvest_start: "2026-09-01", harvest_end: "2026-09-30", status: "growing", provenance: prov("verified"), ...stamps,
}])

export const harvests = list([{
  id: "h1", crop_cycle_id: "cy1", kind: "expected", quantity_kg: "3200.00", is_current: true, reported_for: null,
  village_id: VILLAGE, provenance: prov("unverified"), ...stamps,
}])

export const plots = list([{
  id: "pl1", farm_id: "f1", label: "Kipande A", area_ha: "1.6000", latitude: null, longitude: null,
  village_id: VILLAGE, provenance: prov("verified"), ...stamps,
}])

export const farms = list([{
  id: "f1", label: "Shamba la Neema", household_id: null, latitude: "-7.1", longitude: "35.2",
  managers: [{ person_id: "p1", is_primary: true }], village_id: VILLAGE, provenance: prov("verified"), ...stamps,
}])

export const pueRequests = list([{
  id: "r1", village_id: VILLAGE, person: { id: "p1", name: "Neema Mushi" }, farm_id: null, farm: null,
  equipment: { id: "e1", code: "MILL", name_en: "Maize mill", name_sw: "Kinu" }, acquisition: "buy", quantity: 1,
  hours_per_day: null, days_per_week: null, purpose: null, status: "under_review", submitted_at: "2026-10-01T08:00:00Z",
  decided_at: null, decided_by: null, decision_note: null,
  estimate: { est_power_kw: "15.000", est_kwh_per_day: "120.000", est_kwh_per_week: "720.000" },
  provenance: prov("unverified"), ...stamps,
}])

/** A list asked with `page_size=1` for its `meta.total` only. */
export const total = (n: number): MockReply => ({ body: { data: [], meta: meta(n, 1) } })

export const refused = (status: number, message: string): MockReply => ({ status, body: { message } })
