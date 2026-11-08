/**
 * Pictures for read results. The server picks one from the operation id and the schema-checked
 * response; the model never chooses or writes markup. Fields are picked, never calculated.
 */
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const ROWS = 5

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
export type ResultView =
  | {
      kind: "sales"
      from: string
      to: string
      source: string | null
      rows: { currency: string; decimals: number; gross: number; refunded: number; orders: number }[]
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
          ? [{ currency, decimals, gross, refunded, orders }]
          : []
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
