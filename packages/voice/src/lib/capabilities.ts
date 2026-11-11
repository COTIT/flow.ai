/**
 * What the voice assistant offers to do, curated: start-screen chips, the follow-up chip and the
 * "What can you do?" answer all come from here. Text lives in `voice.capabilities.<id>`.
 * Every op must be executable in the voice catalog (capabilities.test.ts enforces it).
 * Enable after merge: attention_summary (general, priority 1), users_remove, invite with team,
 * overdue invoice filter.
 */
export type CapabilityModule =
  "invoices" | "orders" | "products" | "stock" | "till" | "settings"

export type Capability = {
  id: string
  /** Operations the capability runs; the member needs the rights of all of them. */
  ops: string[]
  module: CapabilityModule
  /** 1 = most valuable. */
  priority: 1 | 2 | 3
  /** Completed operations after which this is the one follow-up suggestion. */
  after?: string[]
}

export const CAPABILITIES: Capability[] = [
  {
    id: "open_invoices",
    ops: ["invoices_list"],
    module: "invoices",
    priority: 1,
  },
  {
    id: "email_invoice",
    ops: ["invoices_send"],
    module: "invoices",
    priority: 1,
    after: ["invoices_get", "orders_invoice"],
  },
  {
    id: "resend_invoice",
    ops: ["invoices_resend"],
    module: "invoices",
    priority: 2,
  },
  {
    id: "record_payment",
    ops: ["invoice_payments_create"],
    module: "invoices",
    priority: 2,
  },
  { id: "create_order", ops: ["orders_create"], module: "orders", priority: 1 },
  {
    id: "weekly_sales",
    ops: ["orders_sales_summary"],
    module: "orders",
    priority: 1,
  },
  { id: "todays_orders", ops: ["orders_list"], module: "orders", priority: 2 },
  {
    id: "invoice_order",
    ops: ["orders_invoice"],
    module: "orders",
    priority: 2,
    after: ["orders_get"],
  },
  {
    id: "change_price",
    ops: ["products_update"],
    module: "products",
    priority: 1,
    after: ["products_get"],
  },
  {
    id: "find_product",
    ops: ["products_list"],
    module: "products",
    priority: 2,
  },
  { id: "check_stock", ops: ["stock_list"], module: "stock", priority: 1 },
  { id: "below_zero", ops: ["stock_negative"], module: "stock", priority: 2 },
  {
    id: "add_stock",
    ops: ["stock_mutate"],
    module: "stock",
    priority: 2,
    after: ["stock_list", "stock_negative"],
  },
  { id: "till_sales", ops: ["till_sales_list"], module: "till", priority: 2 },
  {
    id: "invite_colleague",
    ops: ["invites_send"],
    module: "settings",
    priority: 2,
    after: ["users_list"],
  },
  { id: "team_members", ops: ["users_list"], module: "settings", priority: 3 },
]

/** A curated entry the catalog can run, with the rights its operations need. */
export type Available = Omit<Capability, "ops"> & {
  rights: { allOf: string[]; anyOf: string[] }
}

const MODULES = new Set(CAPABILITIES.map((c) => c.module))

/** The module of the office page the assistant was opened from; only known modules. */
export function moduleFor(path?: string | null): CapabilityModule | undefined {
  const segment = path?.match(/^\/([a-z]+)(?:[/?]|$)/)?.[1]
  return MODULES.has(segment as CapabilityModule)
    ? (segment as CapabilityModule)
    : undefined
}

/** Entries the member may use; `may` is useRights().may (admins may all). */
export const allowedFor = (
  list: Available[],
  may: (right: string) => boolean
) =>
  list.filter(
    (c) =>
      c.rights.allOf.every(may) &&
      (!c.rights.anyOf.length || c.rights.anyOf.some(may))
  )

/** By priority, each priority rotated by `day` so returning users see others. */
export function rotated(list: Available[], day: number) {
  return ([1, 2, 3] as const).flatMap((priority) => {
    const group = list.filter((c) => c.priority === priority)
    const n = group.length ? day % group.length : 0
    return [...group.slice(n), ...group.slice(0, n)]
  })
}

/** Current page's capabilities first, then one per other module, then the rest. */
export function pickSuggestions(
  list: Available[],
  module: CapabilityModule | undefined,
  day: number,
  count = 4
) {
  const ordered = rotated(list, day)
  const seen = new Set<string>([module ?? ""])
  const spread = ordered.filter(
    (c) => !seen.has(c.module) && Boolean(seen.add(c.module))
  )
  const local = ordered.filter((c) => c.module === module)
  return [...new Set([...local, ...spread, ...ordered])].slice(0, count)
}

/** The one follow-up after a completed operation, from the fixed `after` mapping. */
export const followUpFor = (list: Available[], operationId?: string) =>
  operationId ? list.find((c) => c.after?.includes(operationId)) : undefined
