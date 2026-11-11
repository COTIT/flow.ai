/**
 * Pictures for read results. The server picks one from the operation id and the schema-checked
 * response; the model never chooses or writes markup. Fields are picked, never calculated.
 */
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const ROWS = 5
const BREAKDOWN_ROWS = 8

export type Money = { minor: number; decimals: number; currency: string }
export type RecordRow = {
  id: string
  label: string
  detail: string | null
  status: string | null
  money: Money | null
  stock: number | null
  href: string
}
export type AttentionItem = {
  label: string
  detail: string | null
  status: string | null
  money: Money | null
  level: string | null
  at: string | null
  attempts: number | null
  href: string | null
}
export type ResultView = (
  | {
      kind: "sales"
      from: string
      to: string
      source: string | null
      // net/vat: before refunds; null when the API sent no VAT split.
      rows: {
        currency: string
        decimals: number
        gross: number
        net: number | null
        vat: number | null
        // null: the source has no refund split (daily series).
        refunded: number | null
        orders: number
      }[]
    }
  | {
      kind: "sales_daily"
      from: string
      to: string
      time_zone: string | null
      currencies: { currency: string; decimals: number; days: { date: string; gross: number; net: number; orders: number }[] }[]
    }
  | { kind: "records"; resource: Resource; total: number; href: string; rows: RecordRow[] }
  | {
      kind: "lot_origin"
      title: string | null
      origin: { country: string; region: string | null; place: string | null } | null
      materials: { name: string; origin: string | null; share: number | null }[]
    }
  | {
      kind: "trail"
      events: { kind: string; from: string | null; to: string | null; at: string; by: string | null }[]
    }
  | {
      kind: "attention"
      sections: { key: string; counts: Record<string, number>; items: AttentionItem[] }[]
    }
  | {
      kind: "breakdown"
      by: "products" | "customers"
      from: string
      to: string
      // Ranked as the API ranked them; label null = sales without a customer (walk-ins).
      currencies: { currency: string; decimals: number; rows: { label: string | null; gross: number; orders: number }[] }[]
    }
  | {
      kind: "comparison"
      from: string
      to: string
      base_from: string
      base_to: string
      // Gross per period and the API's own change (basis points; null when the base is 0).
      currencies: { currency: string; decimals: number; current: number; base: number; change_bp: number | null }[]
    }
) & { presentation?: Kind }

type Resource = "orders" | "invoices" | "products" | "lots"
type Obj = Record<string, unknown>

const obj = (v: unknown): v is Obj => !!v && typeof v === "object" && !Array.isArray(v)
const text = (v: unknown) =>
  typeof v === "string" && v.trim() ? v.trim().slice(0, 120) : null
const int = (v: unknown) => (Number.isSafeInteger(v) ? (v as number) : null)
const id = (v: unknown) => (typeof v === "string" && UUID.test(v) ? v.toLowerCase() : null)
const money = (minor: unknown, decimals: unknown, currency: unknown): Money | null => {
  const m = int(minor), d = int(decimals), c = text(currency)
  return m === null || d === null || !c ? null : { minor: m, decimals: d, currency: c }
}
const rows = (v: unknown) => (Array.isArray(v) ? v.filter(obj) : null)

function record(resource: Resource, row: Obj): RecordRow | null {
  const key = id(row.id)
  const label =
    resource === "orders" ? text(row.number)
    : resource === "invoices" ? text(row.invoice_nr) ?? text(row.reference)
    : resource === "lots" ? text(row.code)
    : text(row.name)
  if (!key || !label) return null
  const cash = obj(row.money) ? row.money : {}
  return {
    id: key,
    label,
    detail:
      resource === "orders" ? (obj(row.customer) ? text(row.customer.name) : null)
      : resource === "invoices" ? text(row.customer_name)
      : resource === "products" ? text(row.product_nr)
      : null,
    status: text(row.status),
    money:
      resource === "orders" ? money(row.total, row.decimals, row.currency)
      : resource === "invoices" ? money(cash.open, cash.decimals, cash.currency)
      : null,
    stock:
      resource === "products" && typeof row.total_available_stock === "number" && Number.isFinite(row.total_available_stock)
        ? row.total_available_stock
        : null,
    href: `/${resource}/${key}`,
  }
}

const LISTS: Record<string, Resource> = {
  orders_list: "orders",
  invoices_list: "invoices",
  products_list: "products",
  lots_list: "lots",
}

function attentionItem(key: string, row: Obj): AttentionItem | null {
  const ref = (v: unknown) => (obj(v) ? v : {})
  const currency = ref(row.currency)
  const base = { detail: null, status: null, money: null, level: null, at: null, attempts: null, href: null }
  const record = (resource: Resource, label: string | null, extra: Partial<AttentionItem>) => {
    const key = id(row.id)
    return label ? { ...base, ...extra, label, href: key ? `/${resource}/${key}` : null } : null
  }
  if (key === "orders")
    return record("orders", text(row.number), {
      status: text(row.status),
      at: text(row.created_at),
      money: money(row.total, currency.decimals, currency.code),
    })
  if (key === "invoices")
    return record("invoices", text(row.number), {
      status: text(row.status),
      detail: text(row.customer_name),
      at: text(row.due_date),
      money: money(row.open, currency.decimals, currency.code),
    })
  if (key === "stock") {
    const product = ref(row.product), product_id = id(product.id), label = text(product.name)
    return label
      ? { ...base, label, detail: text(ref(row.warehouse).name), level: text(row.level), href: product_id ? `/products/${product_id}` : null }
      : null
  }
  if (key === "till_shifts") {
    const label = text(ref(row.register).name)
    return label ? { ...base, label, at: text(row.opened_at) } : null
  }
  if (key === "webhooks") {
    const label = text(row.event_type)
    return label
      ? { ...base, label, detail: text(row.last_error), attempts: int(row.attempts), at: text(row.updated_at) }
      : null
  }
  return null
}

const SECTIONS = ["orders", "invoices", "stock", "till_shifts", "lots", "webhooks"]

// Flow noticed kinds the screen knows, and where each item's record lives.
const PRIORITIES: Record<string, Resource | null> = {
  "stock.below_minimum": "products",
  "orders.open": "orders",
  "invoices.overdue": "invoices",
  "till.open_shifts": null,
  "lots.quarantined": "lots",
  "webhooks.failed": null,
}
const DAY = /^\d{4}-\d{2}-\d{2}$/

function priorityItem(resource: Resource | null, row: Obj): AttentionItem | null {
  const label = text(row.label)
  if (!label) return null
  const cash = obj(row.money) ? row.money : {}
  const ref = id(row.ref_id)
  return {
    label,
    detail: null,
    status: null,
    money: money(cash.amount, cash.decimals, cash.currency),
    level: text(row.available),
    at: text(row.since),
    attempts: null,
    href: resource && ref ? `/${resource}/${ref}` : null,
  }
}

/** One sales view per day (each currency of that day side by side) for the period bar chart; never sums. */
export function dailyAsSales(view: Extract<ResultView, { kind: "sales_daily" }>): Extract<ResultView, { kind: "sales" }>[] {
  const dates = [...new Set(view.currencies.flatMap((c) => c.days.map((d) => d.date)))].sort()
  return dates.map((date) => ({
    kind: "sales",
    from: date,
    to: date,
    source: null,
    rows: view.currencies.flatMap((c) => {
      const day = c.days.find((d) => d.date === date)
      return day
        ? [{ currency: c.currency, decimals: c.decimals, gross: day.gross, net: day.net, vat: null, refunded: null, orders: day.orders }]
        : []
    }),
  }))
}

export function resultView(operationId: string, response: unknown): ResultView | undefined {
  const body = obj(response) ? response : undefined
  if (!body) return
  const resource = LISTS[operationId]
  if (resource) {
    const list = rows(body.data)
    if (!list) return
    return {
      kind: "records",
      resource,
      total: int(obj(body.meta) ? body.meta.total : undefined) ?? list.length,
      href: `/${resource}`,
      rows: list.flatMap((row) => record(resource, row) ?? []).slice(0, ROWS),
    }
  }
  const data = body.data
  if (operationId === "orders_sales_summary" && obj(data)) {
    const list = rows(data.currencies)
    const from = text(data.from), to = text(data.to)
    if (!list || !from || !to) return
    return {
      kind: "sales",
      from,
      to,
      source: text(data.source),
      rows: list.flatMap((row) => {
        const currency = text(row.currency), decimals = int(row.decimals)
        const gross = int(row.gross_total), refunded = int(row.refunded_total), orders = int(row.orders)
        return currency && decimals !== null && gross !== null && refunded !== null && orders !== null
          ? [{ currency, decimals, gross, net: int(row.net_total), vat: int(row.vat_total), refunded, orders }]
          : []
      }),
    }
  }
  if (operationId === "orders_sales_daily" && obj(data)) {
    const list = rows(data.currencies)
    const from = text(data.from), to = text(data.to)
    if (!list || !from || !to) return
    return {
      kind: "sales_daily",
      from,
      to,
      time_zone: text(data.time_zone),
      currencies: list.flatMap((row) => {
        const currency = text(row.currency), decimals = int(row.decimals)
        if (!currency || decimals === null) return []
        const days = (rows(row.days) ?? []).flatMap((d) => {
          const date = text(d.date), gross = int(d.gross), net = int(d.net), orders = int(d.orders)
          return date && DAY.test(date) && gross !== null && net !== null && orders !== null ? [{ date, gross, net, orders }] : []
        })
        return [{ currency, decimals, days }]
      }),
    }
  }
  if (operationId === "attention_priorities" && obj(data)) {
    const groups = rows(data.groups)
    if (!groups) return
    return {
      kind: "attention",
      // API order is the priority order; unknown kinds are left out, restricted ones never arrive.
      sections: groups.flatMap((g) => {
        const kind = text(g.kind)
        if (!kind || !(kind in PRIORITIES)) return []
        const counts = Object.fromEntries(
          (["open", "acknowledged", "snoozed"] as const).flatMap((k) => {
            const n = int(g[`${k}_count`])
            return n === null ? [] : [[k, n]]
          })
        ) as Record<string, number>
        const items = (rows(g.items) ?? []).flatMap((row) => priorityItem(PRIORITIES[kind] ?? null, row) ?? []).slice(0, ROWS)
        return [{ key: kind.replace(".", "_"), counts, items }]
      }),
    }
  }
  if (operationId === "orders_sales_compare" && obj(data)) {
    const list = rows(data.currencies)
    const now = obj(data.period) ? data.period : {}, before = obj(data.compare_to_period) ? data.compare_to_period : {}
    const from = text(now.from), to = text(now.to), base_from = text(before.from), base_to = text(before.to)
    if (!list || !from || !to || !base_from || !base_to) return
    return {
      kind: "comparison",
      from,
      to,
      base_from,
      base_to,
      currencies: list.flatMap((row) => {
        const currency = text(row.currency), decimals = int(row.decimals)
        const current = obj(row.current) ? int(row.current.gross_total) : null
        const base = obj(row.base) ? int(row.base.gross_total) : null
        return currency && decimals !== null && current !== null && base !== null
          ? [{ currency, decimals, current, base, change_bp: int(row.gross_change_bp) }]
          : []
      }),
    }
  }
  const by = operationId === "products_top_sellers" ? "products" : operationId === "orders_top_customers" ? "customers" : null
  if (by && obj(data)) {
    const list = rows(data.currencies)
    const from = text(data.from), to = text(data.to)
    if (!list || !from || !to) return
    return {
      kind: "breakdown",
      by,
      from,
      to,
      currencies: list.flatMap((row) => {
        const currency = text(row.currency), decimals = int(row.decimals)
        if (!currency || decimals === null) return []
        const ranked = (rows(row.rows) ?? []).flatMap((r) => {
          const label =
            by === "products" ? text(r.title) ?? (obj(r.product) ? text(r.product.name) : null)
            : obj(r.relation) ? text(r.relation.name) : null
          const gross = int(r.gross_total), orders = int(r.orders)
          return label && gross !== null && orders !== null ? [{ label, gross, orders }] : []
        }).slice(0, BREAKDOWN_ROWS)
        const walkIn = obj(row.without_customer) ? row.without_customer : {}
        const walkGross = int(walkIn.gross_total), walkOrders = int(walkIn.orders)
        return [{
          currency,
          decimals,
          rows: walkGross !== null && walkOrders ? [...ranked, { label: null, gross: walkGross, orders: walkOrders }] : ranked,
        }]
      }),
    }
  }
  if (operationId === "lots_passport_get" && obj(data)) {
    const passport = obj(data.published) ? data.published : obj(data.draft) ? data.draft : undefined
    if (!passport) return
    const origin = obj(passport.origin) ? passport.origin : undefined
    const country = origin && text(origin.country)
    return {
      kind: "lot_origin",
      title: text(passport.title),
      origin: country ? { country, region: text(origin.region), place: text(origin.place) } : null,
      materials: (rows(passport.materials) ?? []).flatMap((m) => {
        const name = text(m.name)
        return name ? [{ name, origin: text(m.origin), share: int(m.share_bp) }] : []
      }),
    }
  }
  if (operationId === "lots_events") {
    const list = rows(data)
    if (!list) return
    return {
      kind: "trail",
      events: list.slice(0, 6).flatMap((e) => {
        const kind = text(e.kind), at = text(e.created_at)
        return kind && at
          ? [{ kind, from: text(e.from_status), to: text(e.to_status), at, by: obj(e.user) ? text(e.user.name) : null }]
          : []
      }),
    }
  }
  if (operationId === "attention_summary" && obj(data))
    return {
      kind: "attention",
      sections: SECTIONS.flatMap((key) => {
        const section = data[key]
        // A null section is one the caller may not see: hide it, never report an error.
        if (!obj(section)) return []
        const counts = Object.fromEntries(
          Object.entries(section).filter(([, v]) => Number.isSafeInteger(v))
        ) as Record<string, number>
        const items = (rows(section.items) ?? []).flatMap((row) => attentionItem(key, row) ?? []).slice(0, ROWS)
        return [{ key, counts, items }]
      }),
    }
}

/** Every way the screen can draw a result. The agent may ask for one; Office checks it fits. */
export const KINDS = [
  "line", "sparkline", "bar", "hbar", "pie", "donut", "kpi", "delta", "meter", "table", "list", "timeline", "map",
] as const
export type Kind = (typeof KINDS)[number]

/** What a result's data is, whatever read it came from. */
export type Shape = "series" | "totals" | "comparison" | "breakdown" | "share" | "counts" | "rows" | "events"
const SHAPE: Record<ResultView["kind"], Shape> = {
  sales_daily: "series",
  sales: "totals",
  comparison: "comparison",
  breakdown: "breakdown",
  lot_origin: "share",
  attention: "counts",
  records: "rows",
  trail: "events",
}
export const shapeOf = (view: ResultView) => SHAPE[view.kind]

/** The kinds that honestly draw each shape; the first is the default. */
export const SUPPORTS: Record<Shape, readonly Kind[]> = {
  series: ["line", "sparkline", "bar", "table"],
  totals: ["hbar", "bar", "kpi", "table"],
  comparison: ["delta", "bar", "hbar", "kpi", "table"],
  breakdown: ["hbar", "bar", "pie", "donut", "table", "list"],
  share: ["list", "bar", "hbar", "pie", "donut", "meter", "table"],
  counts: ["list", "bar", "hbar", "pie", "donut", "kpi", "table"],
  rows: ["list", "kpi", "table"],
  events: ["timeline", "list", "table"],
}
export const drawnAs = (view: ResultView): Kind => view.presentation ?? SUPPORTS[shapeOf(view)][0]!

// Why a kind cannot draw some data (said by the agent in one sentence), and its nearest kinds.
const NEEDS: Record<Kind, string> = {
  line: "a line needs values over time",
  sparkline: "a trend line needs values over time",
  bar: "bars need an amount per item, day or period",
  hbar: "ranked bars need an amount per item",
  pie: "a pie needs parts of one whole",
  donut: "a donut needs parts of one whole",
  kpi: "a single figure here would have to be added up, and figures are never made up",
  delta: "a change needs two periods to compare",
  meter: "a meter needs a part of a known total",
  table: "a table needs rows",
  list: "a list needs separate records or items",
  timeline: "a timeline needs dated events",
  map: "a map needs a location, and this result has none",
}
const NEAREST: Record<Kind, Kind[]> = {
  line: ["sparkline", "bar"],
  sparkline: ["line", "bar"],
  bar: ["hbar", "line"],
  hbar: ["bar"],
  pie: ["donut", "bar", "hbar"],
  donut: ["pie", "bar", "hbar"],
  kpi: ["delta", "list"],
  delta: ["kpi", "bar"],
  meter: ["hbar", "bar"],
  table: ["list"],
  list: ["table"],
  timeline: ["list", "table"],
  map: ["list", "table"],
}

/** A requested kind if it can draw this data; otherwise the default, why, and the closest fit. */
export function choosePresentation(view: ResultView, asked: unknown) {
  const supports = SUPPORTS[shapeOf(view)]
  const shown = supports[0]!
  if (typeof asked === "string" && supports.includes(asked as Kind))
    return { view: { ...view, presentation: asked === shown ? undefined : (asked as Kind) } as ResultView }
  const known = typeof asked === "string" && (KINDS as readonly string[]).includes(asked)
  return {
    view,
    note: {
      asked: known ? (asked as Kind) : "unknown",
      shown,
      closest: (known ? NEAREST[asked as Kind] : []).find((k) => supports.includes(k)) ?? shown,
      reason: known ? NEEDS[asked as Kind] : "that is not a way this screen can draw results",
    },
  }
}
