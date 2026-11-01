import type { MockReply } from "../../src/api-mock"

export const DEMAND = "61000000-0000-4000-8000-000000000001"
export const OPPORTUNITY = "62000000-0000-4000-8000-000000000001"

const page = (data: unknown[]): MockReply => ({
  body: { data, meta: { current_page: 1, from: data.length ? 1 : null, to: data.length || null, last_page: 1, per_page: 100, total: data.length } },
})
const stamps = { created_at: "2026-10-01T00:00:00Z", updated_at: "2026-10-01T00:00:00Z" }
const ops = { id: "u-ops", name: "Omari Ops" }
const maize = { id: "c1", code: "MAIZE", name_en: "Maize", name_sw: "Mahindi" }
const ilundo = { id: "v1", name: "Ilundo" }

export const iringa = { id: "b1", name: "Iringa Grain", channel: "direct", contact_note: "Collects from the shed", is_active: true, ...stamps }
export const mbeya = { id: "b2", name: "Mbeya Millers", channel: "afm", contact_note: null, is_active: true, ...stamps }
export const buyers = (...rows: unknown[]) => page(rows)

/** GET .../farm/crops (the demand form's crop picker). */
export const crops = page([{ ...maize, measured_by: "kg", is_active: true, ...stamps }])

export const demand = {
  id: DEMAND,
  buyer: { id: "b1", name: "Iringa Grain" },
  buyer_channel: "direct",
  crop: maize,
  quantity_kg: "9000.00",
  window_start: "2026-09-01",
  window_end: "2026-10-31",
  quality_note: null,
  delivery_point: "Ilundo shed",
  indicative_price_per_kg: { minor: 78000, decimals: 2, currency: "TZS", currency_id: "c-tzs" },
  confidence: "medium",
  status: "open",
  verification: "unverified",
  captured_at: "2026-10-01T08:00:00Z",
  captured_by: ops,
  ...stamps,
}
export const demands = (...rows: unknown[]) => page(rows)

/** GET .../buyer-demands/{d}/matches: one village compared, its opportunity already open. */
export const matches: MockReply = {
  body: {
    data: [
      {
        buyer_demand_id: DEMAND, buyer: { id: "b1", name: "Iringa Grain" }, buyer_name: "Iringa Grain", crop_id: "c1",
        demand_kg: "9000.00", window_start: "2026-09-01", window_end: "2026-10-31", village: ilundo,
        available_kg: "5600.00", committed_kg: "6400.00", coverable_kg: "5600.00", coverage_pct: "62.2",
        opportunity: { id: OPPORTUNITY, status: "proposed" },
      },
    ],
  },
}

const line = (id: string, farmer: string, personId: string, kg: string) => ({
  id, harvest_report_id: `h-${id}`, crop_cycle_id: `cy-${id}`, plot_id: `pl-${id}`, plot_label: `Plot ${id}`, crop_id: "c1",
  farmer: { person_id: personId, name: farmer }, contributed_kg: kg, captured_by: ops, captured_at: "2026-10-02T09:30:00Z",
})

/** The opportunity as ops sees it: every line, with who attached it. */
export const opportunity = {
  id: OPPORTUNITY,
  demand: { id: DEMAND, buyer: { id: "b1", name: "Iringa Grain" }, quantity_kg: "9000.00", window_start: "2026-09-01", window_end: "2026-10-31" },
  village: ilundo,
  crop: maize,
  offered_quantity_kg: "6400.00",
  status: "proposed",
  note: null,
  captured_at: "2026-10-02T09:00:00Z",
  captured_by: ops,
  my_contribution_kg: null,
  supply: [line("s1", "Neema Mwakalinga", "p1", "4100.00"), line("s2", "Juma Kalinga", "p2", "2300.00")],
  ...stamps,
}
export const one = (data: unknown, status = 200): MockReply => ({ status, body: { data } })

/** GET .../farm/opportunities for the farmer: the API sends only their own line and their share. */
export const farmerOpportunities = page([
  { ...opportunity, status: "shared", my_contribution_kg: "4100.00", supply: [{ ...opportunity.supply[0], captured_by: null }] },
])
export const noOpportunities = page([])

/** GET .../farm/harvest-available for Ilundo maize. */
export const available = page([
  {
    harvest_report_id: "h9", crop_cycle_id: "cy9", village_id: "v1", crop_id: "c1", plot_id: "pl9", plot_label: "Shamba la mto",
    farmer: { person_id: "p3", name: "Joseph Kalinga" }, harvest_start: "2026-09-10", harvest_end: null,
    quantity_kg: "5000.00", confidence: null, verification: "unverified", committed_kg: "1000.00", available_kg: "4000.00",
  },
])

/** POST .../supply refused: the API's over-commitment message, as written. */
export const overCommitted: MockReply = {
  status: 422,
  body: { message: "over-commitment: 5000.00 kg available, 1000.00 kg already committed, 4500.00 kg requested" },
}
