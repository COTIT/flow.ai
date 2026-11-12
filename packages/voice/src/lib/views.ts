/**
 * Pictures for read results. The server picks one from the operation id and the schema-checked
 * response; the model never chooses or writes markup. Fields are picked, never calculated.
 */
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const ROWS = 5
const BREAKDOWN_ROWS = 8
const LINES = 12
const STOCK_ROWS = 20

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
  | {
      kind: "stock"
      // list: GET /products/stocks (per product); bins: one product's stock per bin path.
      scope: "list" | "bins"
      total: number | null
      href: string | null
      groups: { title: string | null; rows: StockRow[] }[]
    }
  | { kind: "search"; groups: { key: SearchKey; rows: SearchHit[] }[] }
  | {
      kind: "document"
      resource: "orders" | "invoices" | "products"
      title: string | null
      status: string | null
      href: string
      facts: Fact[]
      // Which money the lines carry, as the API gives it per line; null: lines are stock rows.
      lineMoney: "excl" | "incl" | null
      lines: { label: string | null; quantity: string | null; money: Money | null }[]
      totals: { key: string; money: Money }[]
    }
  | { kind: "till_sales"; total: number; href: string; rows: TillSale[] }
  | { kind: "till_shifts"; total: number; href: string; rows: TillShift[] }
) & { presentation?: Kind }

/** A till sale as booked. `money` is null when the reply has no currency: no amount is guessed. */
export type TillSale = { label: string; at: string; method: string | null; mismatch: boolean; money: Money | null; href: string | null }
/** A drawer shift; `state` follows the API's own `difference` sign once closed. */
export type TillShift = {
  opened: string
  closed: string | null
  state: "open" | "closed" | "short" | "over" | "even"
  expected: Money | null
  counted: Money | null
  difference: Money | null
}

/** Quantities are the API's decimal text, shown as written. `available` is null where the API has none. */
export type StockRow = {
  label: string | null
  detail: string | null
  available: string | null
  level: string
  reserved: string
  minimum: number | null
  href: string | null
}
export type SearchKey = "products" | "orders" | "invoices" | "lots" | "relations"
export type SearchHit = { label: string; detail: string | null; status: string | null; money: Money | null; href: string | null }
/** One header fact of a record: text as the API wrote it, or a date to format. */
export type Fact =
  | { key: string; text: string }
  | { key: string; date: string }
  // A major-unit decimal string as the API wrote it, with the currency it is in.
  | { key: string; price: string; currency: string; decimals: number }

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

const decimal = (v: unknown) => (typeof v === "string" && /^-?\d+(\.\d+)?$/.test(v) ? v : null)
/** "12.5000" → "12.50": only zeros past two places are dropped, so the price is never rounded. */
const price = (v: unknown) => decimal(v)?.replace(/^(-?\d+\.\d\d\d*?)0+$/, "$1") ?? null
const name = (v: unknown, key = "name") => (obj(v) ? text(v[key]) : null)
const joined = (...parts: (string | null)[]) => parts.filter(Boolean).join(" › ") || null

/** Rows grouped by warehouse, in the API's order. */
function byWarehouse(list: Obj[], row: (r: Obj) => StockRow | null, warehouse: (r: Obj) => string | null) {
  const groups: { title: string | null; rows: StockRow[] }[] = []
  for (const r of list) {
    const parsed = row(r)
    if (!parsed) continue
    const title = warehouse(r)
    const group = groups.find((g) => g.title === title) ?? groups[groups.push({ title, rows: [] }) - 1]!
    group.rows.push(parsed)
  }
  return groups
}

function stockView(operationId: string, body: Obj): ResultView | undefined {
  const list = rows(body.data)
  if (!list) return
  if (operationId === "stock_list")
    return {
      kind: "stock",
      scope: "list",
      total: int(obj(body.meta) ? body.meta.total : undefined) ?? list.length,
      href: "/stock",
      groups: byWarehouse(
        list.slice(0, STOCK_ROWS),
        (r) => {
          const rel = obj(r.relationships) ? r.relationships : {}
          const product = obj(rel.product) ? rel.product : {}
          const label = text(product.name), level = decimal(r.stock_lvl), reserved = decimal(r.reserved)
          const key = id(product.id)
          if (!label || !level || !reserved) return null
          return {
            label,
            detail: [text(product.product_nr), name(r.lot, "code")].filter(Boolean).join(" · ") || null,
            available: decimal(r.available),
            level,
            reserved,
            minimum: int(r.minimal_stock_level),
            href: key ? `/products/${key}` : null,
          }
        },
        (r) => name(obj(r.relationships) ? r.relationships.warehouse : null)
      ),
    }
  return {
    kind: "stock",
    scope: "bins",
    total: null,
    href: null,
    groups: byWarehouse(
      list.slice(0, STOCK_ROWS),
      (r) => {
        const level = decimal(r.stock_lvl), reserved = decimal(r.reserved)
        if (!level || !reserved) return null
        const shelf = obj(r.shelf) ? r.shelf : null
        const row = shelf && obj(shelf.row) ? shelf.row : null
        return {
          label: joined(
            text(row?.row_name) ?? text(row?.row_code),
            text(shelf?.shelf_name) ?? text(shelf?.shelf_code),
            name(r.bin, "bin_name") ?? name(r.bin, "bin_code")
          ),
          detail: name(r.lot, "code"),
          available: null,
          level,
          reserved,
          minimum: null,
          href: null,
        }
      },
      (r) => name(r.warehouse)
    ),
  }
}

const SEARCH: { key: SearchKey; hit: (r: Obj) => SearchHit | null }[] = [
  { key: "products", hit: (r) => link("products", r, text(r.name), text(r.product_nr), null, null) },
  { key: "orders", hit: (r) => link("orders", r, text(r.number), text(r.reference), text(r.status), money(r.total, r.decimals, r.currency)) },
  { key: "invoices", hit: (r) => link("invoices", r, text(r.number), text(r.customer_name), text(r.status), null) },
  { key: "lots", hit: (r) => link("lots", r, text(r.code), text(r.product_name), null, null) },
  // Office has no relation page: a customer is named, never linked.
  { key: "relations", hit: (r) => (text(r.name) ? { label: text(r.name)!, detail: null, status: null, money: null, href: null } : null) },
]
function link(resource: Resource, r: Obj, label: string | null, detail: string | null, status: string | null, cash: Money | null): SearchHit | null {
  const key = id(r.id)
  return label ? { label, detail, status, money: cash, href: key ? `/${resource}/${key}` : null } : null
}

function documentView(operationId: string, data: Obj): ResultView | undefined {
  const key = id(data.id)
  if (!key) return
  const facts = (list: [string, string | null, "text" | "date"][]) =>
    list.flatMap(([k, v, as]) => (v ? [as === "date" ? { key: k, date: v } : { key: k, text: v }] : []))
  if (operationId === "orders_get") {
    const cash = (minor: unknown) => money(minor, data.decimals, data.currency)
    const number = text(data.number), total = cash(data.total), subtotal = cash(data.subtotal), vat = cash(data.vat)
    if (!number || !total || !subtotal || !vat) return
    return {
      kind: "document",
      resource: "orders",
      title: number,
      status: text(data.status),
      href: `/orders/${key}`,
      facts: facts([
        ["customer", text(data.customer_name), "text"],
        ["created", text(data.created_at), "date"],
        ["picked", text(data.picked_at), "date"],
        ["paid", text(data.paid_at), "date"],
      ]),
      lineMoney: "excl",
      lines: (rows(data.lines) ?? []).slice(0, LINES).map((l) => ({ label: text(l.title), quantity: decimal(l.quantity), money: cash(l.line_excl) })),
      totals: [{ key: "subtotal", money: subtotal }, { key: "vat", money: vat }, { key: "total", money: total }],
    }
  }
  if (operationId === "invoices_get") {
    const m = obj(data.money) ? data.money : null
    const cash = (minor: unknown) => (m ? money(minor, m.decimals, m.currency) : null)
    const total = cash(m?.total), paid = cash(m?.paid), open = cash(m?.open)
    if (!total || !paid || !open) return
    return {
      kind: "document",
      resource: "invoices",
      title: text(data.invoice_nr),
      status: text(data.status),
      href: `/invoices/${key}`,
      facts: facts([
        ["reference", obj(data.customer_ref) ? text(data.customer_ref.reference) : null, "text"],
        ["due", text(data.due_date), "date"],
      ]),
      lineMoney: "incl",
      lines: (rows(data.items) ?? []).slice(0, LINES).map((l) => ({ label: text(l.title), quantity: decimal(l.quantity), money: cash(l.total_minor) })),
      totals: [{ key: "total", money: total }, { key: "paid", money: paid }, { key: "open", money: open }],
    }
  }
  const title = text(data.name)
  if (!title) return
  const count = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? String(v) : null)
  const code = typeof data.currency === "string" && /^[A-Z]{3}$/.test(data.currency) ? data.currency : null
  const places = int(data.decimals)
  const amount = decimal(data.price_incl)
  const currency = code && places !== null && places >= 0 && places <= 4 && amount ? { price: amount, currency: code, decimals: places } : null
  return {
    kind: "document",
    resource: "products",
    title,
    status: text(data.status),
    href: `/products/${key}`,
    facts: [
      ...facts([
        ["productNr", text(data.product_nr), "text"],
        ["ean", text(data.ean13), "text"],
      ]),
      // With its currency when the reply has one; else the trimmed text, never a guessed symbol.
      ...(currency ? [{ key: "priceIncl", ...currency }] : facts([["priceIncl", price(data.price_incl), "text"]])),
      ...facts([
        ["available", count(data.total_available_stock), "text"],
        ["reserved", count(data.total_reserved_stock), "text"],
      ]),
    ],
    lineMoney: null,
    lines: (rows(data.stocks) ?? []).slice(0, LINES).map((r) => ({
      label: joined(name(r.warehouse), name(r.bin, "bin_name") ?? name(r.bin, "bin_code")),
      quantity: decimal(r.stock_lvl),
      money: null,
    })),
    totals: [],
  }
}

export function resultView(operationId: string, response: unknown): ResultView | undefined {
  const body = obj(response) ? response : undefined
  if (!body) return
  if (operationId === "stock_list" || operationId === "stock_with_bins") return stockView(operationId, body)
  if (operationId === "till_sales_list" || operationId === "till_shifts_list") {
    const list = rows(body.data)
    if (!list) return
    const total = int(obj(body.meta) ? body.meta.total : undefined) ?? list.length
    if (operationId === "till_sales_list")
      return {
        kind: "till_sales",
        total,
        href: "/till/sales",
        rows: list.slice(0, STOCK_ROWS).flatMap((r) => {
          const label = text(r.number), at = text(r.sold_at), order = id(r.order_id)
          return label && at
            ? [{ label, at, method: text(r.method), mismatch: (int(r.price_mismatch) ?? 0) !== 0, money: money(r.total, r.decimals, r.currency), href: order ? `/orders/${order}` : null }]
            : []
        }),
      }
    return {
      kind: "till_shifts",
      total,
      href: "/till/shifts",
      rows: list.slice(0, STOCK_ROWS).flatMap((r) => {
        const opened = text(r.opened_at)
        if (!opened) return []
        const closed = text(r.closed_at), difference = int(r.difference)
        const cash = (minor: unknown) => (minor === null ? null : money(minor, r.decimals, r.currency))
        return [{
          opened,
          closed,
          state: !closed ? "open" as const
            : difference === null ? "closed" as const
            : difference < 0 ? "short" as const
            : difference > 0 ? "over" as const
            : "even" as const,
          expected: cash(r.expected),
          counted: cash(r.counted),
          difference: cash(r.difference),
        }]
      }),
    }
  }
  if (operationId === "search_quick" && obj(body.data)) {
    const data = body.data
    return {
      kind: "search",
      // A null group is one the caller may not list: left out, never reported as empty.
      groups: SEARCH.flatMap(({ key, hit }) => {
        const list = rows(data[key])
        return list ? [{ key, rows: list.flatMap((r) => hit(r) ?? []) }] : []
      }),
    }
  }
  if (["orders_get", "invoices_get", "products_get"].includes(operationId) && obj(body.data))
    return documentView(operationId, body.data)
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
export type Shape =
  | "series" | "totals" | "comparison" | "breakdown" | "share" | "counts" | "rows" | "events"
  | "quantities" | "hits" | "document"
const SHAPE: Record<ResultView["kind"], Shape> = {
  sales_daily: "series",
  sales: "totals",
  comparison: "comparison",
  breakdown: "breakdown",
  lot_origin: "share",
  attention: "counts",
  records: "rows",
  trail: "events",
  stock: "quantities",
  till_sales: "rows",
  till_shifts: "rows",
  search: "hits",
  document: "document",
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
  quantities: ["list", "hbar", "bar", "table"],
  hits: ["list", "table"],
  document: ["list", "table", "kpi"],
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
