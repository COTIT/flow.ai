import type { MockReply } from "../../src/api-mock"

const meta = (total: number) => ({ current_page: 1, from: total ? 1 : null, to: total || null, last_page: 1, per_page: 100, total })

export const ilundoCapacity = { capacity_kw: "500.00", basis: "planned", simultaneity_factor: "0.600", source_note: "DEMO", effective_from: "2026-01-15" }

const village = (over: Record<string, unknown>) => ({
  latitude: null,
  longitude: null,
  primary_language: "sw",
  created_at: "2026-01-15T00:00:00Z",
  updated_at: "2026-01-15T00:00:00Z",
  ...over,
})

export const ilundo = village({ id: "v1", code: "ILUNDO", name: "Ilundo", capacity: ilundoCapacity })
export const mlowa = village({
  id: "v2", code: "MLW", name: "Mlowa", latitude: "-7.910000", longitude: "35.620000",
  capacity: { capacity_kw: "250.00", basis: "planned", simultaneity_factor: "0.650", source_note: null, effective_from: "2026-11-01" },
})

/** GET /workspace/w1/farm/villages, capacity embedded (ops sees every village's). */
export const villageList = (...rows: unknown[]): MockReply => ({ body: { data: rows, meta: meta(rows.length) } })

/** POST .../villages refused: the factor is out of range (the API's words, per field). */
export const factorRefused: MockReply = {
  status: 422,
  body: {
    message: "The given data was invalid.",
    errors: { "capacity.simultaneity_factor": ["simultaneity factor must be more than 0 and at most 1"] },
  },
}
