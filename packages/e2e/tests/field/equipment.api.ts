import type { MockReply } from "../../src/api-mock"

export const MILL = "51000000-0000-4000-8000-000000000001"

const TZS = (minor: number) => ({ minor, decimals: 2, currency: "TZS", currency_id: "c-tzs" })
const page = (data: unknown[]): MockReply => ({
  body: { data, meta: { current_page: 1, from: data.length ? 1 : null, to: data.length || null, last_page: 1, per_page: 100, total: data.length } },
})

export const milling = { id: "cat1", code: "MILLING", name_en: "Milling", name_sw: "Kusaga", is_active: true, created_at: "2026-10-01T00:00:00Z", updated_at: "2026-10-01T00:00:00Z" }

export const mill = {
  id: MILL,
  category: { id: "cat1", code: "MILLING", name_en: "Milling", name_sw: "Kusaga" },
  code: "MILL-500",
  name_en: "Maize mill",
  name_sw: "Mashine ya kusaga",
  rated_power_kw: "15.000",
  typical_hours_per_day: "6.00",
  typical_days_per_week: "5.0",
  can_rent: true,
  can_buy: true,
  indicative_price: TZS(2200000000),
  indicative_rent_per_day: TZS(5000000),
  is_active: true,
  created_at: "2026-10-01T00:00:00Z",
  updated_at: "2026-10-01T00:00:00Z",
}

/** GET .../farm/equipment-categories */
export const categories = (...rows: unknown[]) => page(rows)
/** GET .../farm/equipment */
export const equipment = (...rows: unknown[]) => page(rows)
export const one = (data: unknown): MockReply => ({ body: { data } })

/** GET /workspace/w1/currencies: TZS is the account's default. */
export const currencies: MockReply = page([
  { id: "c-tzs", name: "Tanzanian shilling", asset_type: "tzs", symbol_prefix: null, symbol_suffix: "TZS", decimals: 2, default_currency: true, active: true, meta: null, created_at: "2026-09-01T00:00:00Z", last_updated_at: "2026-09-01T00:00:00Z" },
])

/** POST .../farm/equipment-categories refused: the code is taken. */
export const codeTaken: MockReply = {
  status: 422,
  body: { message: "The given data was invalid.", errors: { code: ["this code is already used in this workspace"] } },
}
