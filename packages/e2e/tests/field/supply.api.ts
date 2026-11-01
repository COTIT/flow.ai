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

/** The API's refusal when the shop revoked the link after the screen loaded (supply-chain.md). */
export const notLive = { message: "The supply link is not live." }
