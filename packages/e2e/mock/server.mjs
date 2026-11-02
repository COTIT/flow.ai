// Mock of the Genesy storefront API for e2e runs of server-rendered apps (the shop).
// No dependencies; listens on 127.0.0.1 only. Signed file links point back at this server.
import console from "node:console"
import { randomUUID } from "node:crypto"
import { createServer } from "node:http"
import { URL } from "node:url"

import {
  APP_KEY,
  LOT,
  MOCK_ORIGIN,
  MOCK_PORT,
  LATE_PAYMENT_EMAIL,
  NO_PROVIDER_EMAIL,
  PRODUCT,
  SECRET_BODY,
  STOCK_LIMIT,
  VARIANT,
} from "./fixtures.mjs"

const link = (name) => ({
  url: `${MOCK_ORIGIN}/files/${name}.svg`,
  expires_at: new Date(Date.now() + 10 * 60_000).toISOString(),
})

const variant = (id, sku, gross, size, isDefault) => ({
  id,
  sku,
  barcode: null,
  ean13: null,
  is_default: isDefault,
  price_gross: gross,
  price_net: gross,
  options: [{ type: "Size", value: size }],
})

const products = [
  {
    id: PRODUCT.honey,
    name: "Ilundo Wild Forest Honey",
    brand: "Ilundo",
    type: "physical",
    short_description:
      "Raw honey from the miombo woodlands of Iringa, Tanzania.",
    description:
      "Harvested by the Ilundo beekeepers from traditional log hives.",
    ean13: null,
    barcode: null,
    price_gross: 1499,
    price_net: 1499,
    currency: "USD",
    currency_decimals: 2,
    available_stock: "42.000",
    variants: [
      variant(VARIANT.honey250, "ILU-250", 1499, "250 g", true),
      variant(VARIANT.honey500, "ILU-500", 2499, "500 g", false),
    ],
  },
  {
    id: PRODUCT.wax,
    name: "Beeswax Block",
    brand: "Ilundo",
    type: "physical",
    short_description: "Pure beeswax, sun-melted.",
    description: null,
    ean13: null,
    barcode: null,
    price_gross: 900,
    price_net: 900,
    currency: "USD",
    currency_decimals: 2,
    available_stock: "0.000",
    variants: [variant(VARIANT.wax, "WAX-1", 900, "200 g", true)],
  },
]

const lot = (code, status, farmers) => ({
  code: "L-2026-014",
  public_code: code,
  status,
  produced_on: "2026-07-14",
  best_before: "2028-07-14",
  expires_on: null,
  origin: "Ilundo, Iringa, Tanzania",
  product: {
    name: "Ilundo Wild Forest Honey",
    brand: "Ilundo",
    short_description: null,
  },
  farmers,
  passport: {
    title: "Wild forest honey, harvest July 2026",
    intro:
      "This jar was filled from one harvest of log hives around Ilundo village.",
    banner: link("banner"),
    origin: { country: "TZ", region: "Iringa", place: "Ilundo" },
    map: { lat: -7.77, lng: 35.69, label: "Ilundo miombo forest" },
    materials: [
      { name: "Raw honey", origin: "Ilundo, Tanzania", share_bp: 10000 },
    ],
    certificates: [
      {
        name: "Organic (EU)",
        issuer: "Ecocert",
        number: "TZ-BIO-123",
        valid_until: "2027-12-31",
        file: link("cert"),
      },
    ],
    story: [
      {
        kind: "text",
        title: "The hives",
        body: "Log hives hang high in the miombo trees.",
      },
      { kind: "image", image: link("story"), caption: "Hives above Ilundo" },
      { kind: "facts", items: [{ label: "Hives", value: "312" }] },
      {
        kind: "quote",
        text: "The bees know the forest better than we do.",
        by: "Beekeeper",
      },
    ],
    version: 3,
    published_at: "2026-08-01T09:00:00Z",
  },
})

const lots = {
  [LOT.released]: () => lot(LOT.released, "released", []),
  [LOT.recalled]: () =>
    // Farmer groups per village; the API sends null instead of a count below 3.
    lot(LOT.recalled, "recalled", [
      {
        village: "Ilundo",
        farmers: 12,
        harvest_start: "2026-07-01",
        harvest_end: "2026-07-20",
      },
      {
        village: "Mtera",
        farmers: null,
        harvest_start: "2026-08-02",
        harvest_end: "2026-08-09",
      },
    ]),
}

// Web orders, in memory. Totals here are fixture numbers, not pricing: the real API prices.
const orders = new Map()
const byKey = new Map()
let numbers = 1000
const view = ({
  id,
  number,
  status,
  subtotal,
  vat,
  total,
  expires_at,
  paid_at,
}) => ({
  id,
  number,
  status,
  currency: "USD",
  decimals: 2,
  subtotal,
  vat,
  total,
  expires_at,
  paid_at,
})

const readJson = (req) =>
  new Promise((resolve) => {
    let raw = ""
    req.on("data", (c) => (raw += c))
    req.on("end", () => {
      try {
        resolve(JSON.parse(raw || "{}"))
      } catch {
        resolve({})
      }
    })
  })

function placeOrder(body, key) {
  if (byKey.has(key)) return [201, { data: view(orders.get(byKey.get(key))) }]
  const errors = {}
  let total = 0
  for (const [i, line] of (body.lines ?? []).entries()) {
    const product = products.find((p) => p.id === line.product_id)
    const variant = product?.variants.find(
      (v) => v.id === line.product_variant_id
    )
    if (!product) errors[`lines.${i}.product_id`] = ["Not found."]
    else if (Number(line.quantity) > STOCK_LIMIT)
      errors[`lines.${i}.quantity`] = [
        `Not enough stock: ${STOCK_LIMIT} available.`,
      ]
    else total += (variant ?? product).price_gross * Number(line.quantity)
  }
  if (Object.keys(errors).length)
    return [422, { message: "The given data was invalid.", errors }]
  const order = {
    id: randomUUID(),
    number: `WEB-${++numbers}`,
    status: "awaiting_payment",
    subtotal: total,
    vat: 0,
    total,
    paid_at: null,
    expires_at: new Date(Date.now() + 30 * 60_000).toISOString(),
    noProvider: body.customer?.email === NO_PROVIDER_EMAIL,
    latePayment: body.customer?.email === LATE_PAYMENT_EMAIL,
    reads: 0,
  }
  orders.set(order.id, order)
  byKey.set(key, order.id)
  return [201, { data: view(order) }]
}

const send = (res, status, body, type = "application/json") => {
  res.writeHead(status, { "content-type": type })
  res.end(type === "application/json" ? JSON.stringify(body) : body)
}

const image = (label) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 600"><rect width="1200" height="600" fill="#8b8171"/><text x="600" y="300" fill="#fff" font-size="64" text-anchor="middle">${label}</text></svg>`

createServer(async (req, res) => {
  const path = new URL(req.url, MOCK_ORIGIN).pathname
  if (path === "/health") return send(res, 200, { ok: true })
  // Stand-in for the provider's hosted page: pays at once and sends the customer back.
  const paying = orders.get(path.match(/^\/pay\/([^/]+)$/)?.[1])
  if (paying) {
    Object.assign(paying, { status: "open", paid_at: new Date().toISOString() })
    res.writeHead(302, { location: paying.redirect })
    return res.end()
  }
  if (path.startsWith("/files/"))
    return send(res, 200, image(path.slice(7)), "image/svg+xml")
  if (!path.startsWith("/v1/storefront/"))
    return send(res, 404, { message: "Not found." })
  if (req.headers.authorization !== `Bearer ${APP_KEY}`)
    return send(res, 401, { message: "Unauthenticated." })

  const failing = { message: SECRET_BODY }
  if (path === "/v1/storefront/orders" && req.method === "POST")
    return send(
      res,
      ...placeOrder(await readJson(req), req.headers["idempotency-key"])
    )
  const orderPath = path.match(
    /^\/v1\/storefront\/orders\/([^/]+)(\/payment)?$/
  )
  if (orderPath) {
    const order = orders.get(orderPath[1])
    if (!order) return send(res, 404, { message: "Not found." })
    if (!orderPath[2]) {
      // The provider's webhook "arrives" between the first and second read.
      if (
        order.latePayment &&
        ++order.reads >= 2 &&
        order.status === "awaiting_payment"
      )
        Object.assign(order, {
          status: "open",
          paid_at: new Date().toISOString(),
        })
      return send(res, 200, { data: view(order) })
    }
    if (order.noProvider)
      return send(res, 503, { message: "no payment provider configured" })
    if (order.status !== "awaiting_payment")
      return send(res, 409, { message: "conflict" })
    order.redirect = (await readJson(req)).redirect_url
    return send(res, 200, {
      data: {
        payment_id: `pay_${order.id}`,
        status: "open",
        checkout_url: `${MOCK_ORIGIN}/pay/${order.id}`,
      },
    })
  }
  if (path === "/v1/storefront/products")
    return send(res, 200, {
      data: products,
      meta: {
        current_page: 1,
        from: 1,
        to: products.length,
        last_page: 1,
        per_page: 24,
        total: products.length,
      },
    })
  const product = path.match(/^\/v1\/storefront\/products\/([^/]+)$/)?.[1]
  if (product === PRODUCT.failing) return send(res, 503, failing)
  if (product) {
    const found = products.find((p) => p.id === product)
    return found
      ? send(res, 200, { data: found })
      : send(res, 404, { message: "Not found." })
  }
  const code = path.match(/^\/v1\/storefront\/lots\/([^/]+)$/)?.[1]
  if (code === LOT.failing) return send(res, 503, failing)
  if (code && lots[code]) return send(res, 200, { data: lots[code]() })
  send(res, 404, { message: "Not found." })
}).listen(MOCK_PORT, "127.0.0.1", () =>
  console.log(`e2e mock storefront on ${MOCK_ORIGIN}`)
)
