import type { MockReply } from "../../src/api-mock"

const TZS = (minor: number) => ({ minor, decimals: 2, currency: "TZS", currency_id: "c-tzs" })

export const inputLoan = {
  id: "52000000-0000-4000-8000-000000000001",
  code: "INPUT",
  name_en: "Input loan",
  name_sw: "Mkopo wa pembejeo",
  description_en: "Seed and fertiliser for a season.",
  description_sw: "Mbegu na mbolea kwa msimu.",
  indicative_min: TZS(20000000),
  indicative_max: TZS(150000000),
  is_active: true,
  created_at: "2026-10-01T00:00:00Z",
  updated_at: "2026-10-01T00:00:00Z",
}

export const womenLoan = {
  ...inputLoan,
  id: "52000000-0000-4000-8000-000000000002",
  code: "WOMEN",
  name_en: "Women group loan",
  name_sw: "Mkopo wa kikundi cha wanawake",
  description_en: null,
  description_sw: null,
  indicative_min: TZS(50000000),
  indicative_max: TZS(300000050),
}

/** GET .../farm/loan-products */
export const loans = (...rows: unknown[]): MockReply => ({
  body: { data: rows, meta: { current_page: 1, from: rows.length ? 1 : null, to: rows.length || null, last_page: 1, per_page: 100, total: rows.length } },
})

/** POST .../farm/loan-products refused: the range runs backwards. */
export const rangeBackwards: MockReply = {
  status: 422,
  body: { message: "The given data was invalid.", errors: { indicative_max_minor: ["the maximum must not be below the minimum"] } },
}
