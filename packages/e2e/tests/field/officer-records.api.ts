import type { Page } from "@playwright/test"

import type { MockReply } from "../../src/api-mock"

export const PERSON = "80000000-0000-4000-8000-000000000001"
export const SPOUSE = "80000000-0000-4000-8000-000000000002"
export const HOUSEHOLD = "81000000-0000-4000-8000-000000000001"
export const FARM = "82000000-0000-4000-8000-000000000001"
export const PLOT = "83000000-0000-4000-8000-000000000001"
export const CYCLE = "84000000-0000-4000-8000-000000000001"
export const HARVEST = "85000000-0000-4000-8000-000000000001"
export const DOCUMENT = "86000000-0000-4000-8000-000000000001"
export const MAIZE = "40000000-0000-4000-8000-000000000001"

const at = "2026-09-01T08:00:00Z"
export const provenance = (verification = "unverified", captured_by: { id: string; name: string } | null = null) => ({
  source: "field_verified", verification, confidence: "high", captured_at: at, captured_by,
  evidence_ref: null, verified_at: null, verified_by: null,
})
const stamps = { created_at: at, updated_at: at }

export const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, from: data.length ? 1 : null, to: data.length || null, last_page: 1, per_page: 100, total: data.length },
})

export const person = (over: Record<string, unknown> = {}) => ({
  id: PERSON, given_name: "Neema", family_name: "Mwakalinga", phone: "+255712345678", village_id: "v1",
  provenance: provenance(), ...stamps, ...over,
})
export const spouse = person({ id: SPOUSE, given_name: "Juma", phone: null, provenance: provenance("verified") })

/** Registered by the signed-in officer (u1): the four-eyes rule applies. */
export const household = {
  id: HOUSEHOLD, label: "Mwakalinga household", village_id: "v1", members: [],
  provenance: provenance("unverified", { id: "u1", name: "Salima Officer" }), ...stamps,
}
export const farm = (over: Record<string, unknown> = {}) => ({
  id: FARM, label: "Riverside farm", village_id: "v1", household_id: HOUSEHOLD, managers: [],
  latitude: "-7.710000", longitude: "35.710000", provenance: provenance(), ...stamps, ...over,
})
export const plot = (over: Record<string, unknown> = {}) => ({
  id: PLOT, farm_id: FARM, label: "Riverside", village_id: "v1", area_ha: "1.8000",
  latitude: null, longitude: null, provenance: provenance(), ...stamps, ...over,
})
export const cycle = {
  id: CYCLE, plot_id: PLOT, crop_id: MAIZE, village_id: "v1", season_label: "Masika", status: "growing",
  area_ha: "1.6000", tree_count: null, unit_count: null, planted_on: null,
  harvest_start: "2026-09-01", harvest_end: "2026-09-30", provenance: provenance(), ...stamps,
}
const harvest = {
  id: HARVEST, crop_cycle_id: CYCLE, village_id: "v1", kind: "expected", quantity_kg: "4100.00", is_current: true,
  reported_for: null, provenance: provenance(), ...stamps,
}

/** GET …/persons/{person}/holdings: households with their people's names, farms down to harvest figures. */
export const holdings = (farmLabel = "Riverside farm") => ({
  data: {
    households: [{
      ...household,
      people: [
        { person_id: PERSON, given_name: "Neema", family_name: "Mwakalinga", is_head: true },
        { person_id: SPOUSE, given_name: "Juma", family_name: "Mwakalinga", is_head: false },
      ],
    }],
    farms: [{
      ...farm({ label: farmLabel }),
      plots: [{
        ...plot(),
        cycles: [{ ...cycle, crop: { id: MAIZE, code: "MZ", name_en: "Maize", name_sw: "Mahindi" }, harvests: [harvest] }],
      }],
    }],
  },
})

export const crops: MockReply = {
  body: page([
    { id: MAIZE, code: "MZ", name_en: "Maize", name_sw: "Mahindi", measured_by: "area", is_active: true, ...stamps },
  ]),
}

export const documents: MockReply = {
  body: page([{
    id: DOCUMENT, plot_id: PLOT, village_id: "v1", file_name: "title-deed.pdf", content_type: "application/pdf",
    size_bytes: 2048, uploaded_at: at, uploaded_by: { id: "u1", name: "Salima Officer" },
  }]),
}

/** Every request to one path (any query), so a spec can assert what was sent. */
export async function record(page: Page, method: string, path: string, reply: () => { status?: number; body: unknown }) {
  const seen: URL[] = []
  await page.route(`**/v1${path}*`, async (route) => {
    const url = new URL(route.request().url())
    if (route.request().method() !== method || url.pathname !== `/v1${path}`) return route.fallback()
    seen.push(url)
    const { status = 200, body } = reply()
    await route.fulfill({ status, contentType: "application/json", body: JSON.stringify(body) })
  })
  return seen
}

export const tourSeen = (page: Page) =>
  page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["officer:u1"])))

/** The `/officer` home's reads (per-village totals of the five record lists), all empty: spread into a mockApi table. */
export const officerHome: Record<string, MockReply> = Object.fromEntries(
  ["persons", "households", "farms", "plots", "crop-cycles"].map((kind) => [`GET /workspace/w1/farm/${kind}`, { body: page([]) }]),
)
