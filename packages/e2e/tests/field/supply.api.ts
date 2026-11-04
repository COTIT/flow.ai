import type { MockReply } from "../../src/api-mock"

const meta = (total: number) => ({ current_page: 1, from: 1, to: total, last_page: 1, per_page: 100, total })
const list = (data: unknown[]): MockReply => ({ body: { data, meta: meta(data.length) } })

const link = (id: string, status: string, fields: string[]) => ({
  id,
  direction: "outgoing",
  partner: { id: "w-shop", name: "AFM shop" },
  fields,
  status,
  granted_at: "2026-10-30T08:00:00Z",
  accepted_at: status === "pending" ? null : "2026-10-30T09:00:00Z",
  revoked_at: status === "revoked" ? "2026-10-31T09:00:00Z" : null,
})

/** GET /supply-links: one live, one revoked, and one the shop granted us (not shown on the farm side). */
export const links = list([
  link("l1", "live", ["village", "farmer_count"]),
  link("l0", "revoked", []),
  { ...link("l9", "live", []), direction: "incoming" },
])

/** POST /supply-links reply. */
export const granted: MockReply = { status: 201, body: { data: { ...link("l2", "pending", ["village"]), partner: { id: "w-shop", name: "AFM shop" } } } }

export const batches = list([
  {
    id: "b1",
    client_ref: "c1",
    village_id: "v1",
    crop_id: "crop1",
    price_per_kg: 800000,
    currency_id: "tzs",
    kg_total: "250.00",
    lot: { id: "lot1", code: "ILU-H-2026-01", public_code: "ABCDEFGH23" },
    lines: [],
    created_by: "u1",
    created_at: "2026-10-29T08:00:00Z",
  },
])

export const warehouses = list([{ id: "wh1", name: "Iringa store" }])

/** GET /farm/dispatch/options: the live links, batch lots and stocked warehouses ops may send. */
export const dispatchOptions: MockReply = {
  body: {
    data: {
      links: [{ id: "l1", partner: { id: "w-shop", name: "AFM shop" } }],
      lots: [{ id: "lot1", code: "ILU-H-2026-01" }],
      warehouses: [{ id: "wh1", name: "Iringa store" }],
    },
  },
}

/** Farm ops hold no office rights: the office warehouse list answers 403. */
export const officeOnly: MockReply = { status: 403, body: { message: "This action is unauthorized." } }

/** GET /lots/lot1/stock: per warehouse, 2-decimal text; `available = level - reserved` is the API's. */
export const lotStock: MockReply = {
  body: { data: [{ warehouse_id: "wh1", level: "40.00", reserved: "5.00", available: "35.00" }] },
}

export const transfers = list([
  {
    id: "t1",
    link_id: "l1",
    direction: "outgoing",
    partner: { id: "w-shop", name: "AFM shop" },
    from_lot: { code: "ILU-H-2026-01", public_code: "ABCDEFGH23" },
    quantity: "40.00",
    status: "received",
    dispatched_at: "2026-10-30T10:00:00Z",
    to_lot: { id: "lotA", code: "AFM-R-01", public_code: "ZYXWVTSR98" },
    received_quantity: "39.50",
    received_at: "2026-10-31T10:00:00Z",
  },
])

/** POST /lots/{lot}/transfers reply. */
export const dispatched = {
  data: {
    id: "t2",
    link_id: "l1",
    direction: "outgoing",
    partner: { id: "w-shop", name: "AFM shop" },
    from_lot: { code: "ILU-H-2026-01", public_code: "ABCDEFGH23" },
    quantity: "12.50",
    status: "dispatched",
    dispatched_at: "2026-11-01T08:00:00Z",
    to_lot: null,
    received_quantity: null,
    received_at: null,
  },
}

/** A reply lost on the way back: the app may resend under the same Idempotency-Key. */
export const unavailable = { message: "service unavailable" }

/** The API's refusal when the shop revoked the link after the screen loaded (supply-chain.md). */
export const notLive = { message: "The supply link is not live." }

const stamps = { created_at: "2026-10-01T00:00:00Z", updated_at: "2026-10-01T00:00:00Z" }

/** GET .../farm/crops (the buy card's crop picker). */
export const crops = list([{ id: "crop1", code: "COFFEE", name_en: "Coffee", name_sw: "Kahawa", measured_by: "area", is_active: true, ...stamps }])

/** GET /workspace/w1/currencies: TZS is the default. */
export const currencies = list([
  { id: "tzs", name: "Tanzanian shilling", asset_type: "tzs", symbol_prefix: null, symbol_suffix: "TZS", decimals: 2, default_currency: true, active: true, meta: null, created_at: "2026-09-01T00:00:00Z", last_updated_at: "2026-09-01T00:00:00Z" },
])

const rawProduct = (id: string, name: string, suggested: boolean) => ({ id, name, unit: "kg", suggested_for_crop: suggested })

/** GET .../farm/supply-batches/options: one harvest to buy; `raw` picks the products and the right. */
export const batchOptions = (raw: { products: unknown[]; canCreate: boolean }): MockReply => ({
  body: {
    data: {
      harvests: [
        { harvest_report_id: "h1", farmer_name: "Neema Mwakalinga", plot_label: "Shamba A", quantity_kg: "300.00", committed_kg: "100.00", available_kg: "200.00" },
      ],
      raw_products: raw.products,
      can_create_product: raw.canCreate,
    },
  },
})
export const honeyJar = rawProduct("p1", "Honey jar", false)
// Named as the seed names it: brackets in the name must not double up with the unit.
export const rawCoffee = rawProduct("p9", "Coffee cherries (raw)", true)

/** POST .../farm/supply-batches/raw-product reply. */
export const rawProductMade: MockReply = { status: 201, body: { data: { id: "p9", name: "Raw Coffee", unit: "kg" } } }

/** POST .../farm/supply-batches reply. */
export const batchMade: MockReply = {
  status: 201,
  body: { data: { ...(batches.body as { data: Record<string, unknown>[] }).data[0], id: "b2", lot: { id: "lot2", code: "ILU-C-01", public_code: "QWERTYUP23" } } },
}

/** GET .../farm/supply-batches/options with nothing left to buy (all bought, or none verified). */
export const nothingToBuy: MockReply = {
  body: { data: { harvests: [], raw_products: [rawCoffee], can_create_product: true } },
}

/** POST .../farm/supply-batches refusals, worded as the API words them (farm_supply/service.rs). */
export const batchInvalid: MockReply = {
  status: 422,
  body: {
    message: "The given data was invalid.",
    errors: {
      code: ["The code has already been taken."],
      price_per_kg: ["The price per kg must be at least 1."],
      "lines.0.harvest_report_id": ["The harvest report must be the current, verified actual figure of this village and crop."],
    },
  },
}
export const ownVerification: MockReply = { status: 403, body: { message: "A harvest you verified must be bought by someone else." } }
