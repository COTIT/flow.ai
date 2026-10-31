import type { MockReply } from "../../src/api-mock"

export const CYCLE = "70000000-0000-4000-8000-000000000001"
export const PLOT = "71000000-0000-4000-8000-000000000001"
export const MAIZE = "40000000-0000-4000-8000-000000000001"

const provenance = (verification = "unverified", captured_at = "2026-09-01T08:00:00Z") => ({
  source: "farmer_reported",
  verification,
  confidence: "medium",
  captured_at,
  captured_by: null,
  evidence_ref: null,
  verified_at: null,
  verified_by: null,
})

/** GET /workspace/w1/farm/crop-cycles/{cycle}. */
export const cycle: MockReply = {
  body: {
    data: {
      id: CYCLE,
      crop_id: MAIZE,
      plot_id: PLOT,
      village_id: "v1",
      season_label: "Msimu 2026 A",
      status: "growing",
      area_ha: "1.6000",
      tree_count: null,
      unit_count: null,
      planted_on: "2026-03-01",
      harvest_start: "2026-09-01",
      harvest_end: "2026-09-30",
      provenance: provenance(),
      created_at: "2026-09-01T08:00:00Z",
      updated_at: "2026-09-01T08:00:00Z",
    },
  },
}

/** GET /workspace/w1/farm/plots/{plot}: only the label is shown. */
export const plot: MockReply = {
  body: {
    data: {
      id: PLOT, farm_id: "f1", village_id: "v1", label: "Riverside", area_ha: "2.0000",
      latitude: null, longitude: null, provenance: provenance(),
      created_at: "2026-09-01T08:00:00Z", updated_at: "2026-09-01T08:00:00Z",
    },
  },
}

export const crops: MockReply = {
  body: {
    data: [
      { id: MAIZE, code: "MZ", name_en: "Maize", name_sw: "Mahindi", measured_by: "area", is_active: true, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z" },
    ],
    meta: { current_page: 1, from: 1, to: 1, last_page: 1, per_page: 100, total: 1 },
  },
}

const report = (id: string, quantity_kg: string, is_current: boolean, captured_at: string) => ({
  id, crop_cycle_id: CYCLE, village_id: "v1", kind: "expected", quantity_kg, is_current,
  reported_for: null, provenance: provenance("unverified", captured_at),
  created_at: captured_at, updated_at: captured_at,
})

export const FIRST = "72000000-0000-4000-8000-000000000001"
export const REVISED = "72000000-0000-4000-8000-000000000002"

/** The series before the officer's revision: one current expected figure. */
export const before = [report(FIRST, "3200.00", true, "2026-09-01T08:00:00Z")]
/** After it: the new figure is current, the old one stays on record. */
export const after = [
  report(REVISED, "4100.00", true, "2026-10-29T08:00:00Z"),
  report(FIRST, "3200.00", false, "2026-09-01T08:00:00Z"),
]

export const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, from: 1, to: data.length, last_page: 1, per_page: 100, total: data.length },
})
