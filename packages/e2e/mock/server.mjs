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
  BUSY_EMAIL,
  CATEGORY,
  FILES_ORIGIN,
  FILES_PORT,
  CANCEL_PAYMENT_EMAIL,
  CHECKOUT_DOWN_EMAIL,
  EXPIRED_EMAIL,
  FAILING_PAGE,
  FAILING_ORDER,
  LATE_PAYMENT_EMAIL,
  LONG_NAME,
  NO_PROVIDER_EMAIL,
  PROVIDER_DOWN_EMAIL,
  PRODUCT,
  REFUNDED_EMAIL,
  SECRET_BODY,
  STOCK_LIMIT,
  VARIANT,
} from "./fixtures.mjs"

const link = (name) => ({
  // Signed-link shape: a query string, like a B2 presigned URL.
  url: `${FILES_ORIGIN}/files/${name}.svg?X-Amz-Expires=600&X-Amz-Signature=e2e`,
  expires_at: new Date(Date.now() + 10 * 60_000).toISOString(),
})

const categories = [
  [
    CATEGORY.honey,
    "Honey",
    "Raw honey and beeswax from the Ilundo beekeepers.",
  ],
  [CATEGORY.coffee, "Coffee", "Arabica from smallholder farms in Tanzania."],
  [CATEGORY.tea, "Tea", "Roselle and black teas from Tanzania and Kenya."],
  [CATEGORY.spices, "Spices", "Whole and ground spices grown in Tanzania."],
  [CATEGORY.baskets, "Baskets", "Handwoven baskets from East African makers."],
  [CATEGORY.toys, "Toys & games", "Hand-carved animals and bao boards."],
  [CATEGORY.art, "Art", "Originals and prints from East African artists."],
  [CATEGORY.books, "Books", "Stories from the farm and the village."],
  [CATEGORY.apparel, "Apparel", "Cotton shirts that carry the story."],
  [CATEGORY.stickers, "Stickers", "Small things that start a conversation."],
].map(([id, name, description]) => ({ id, name, description }))
const ref = (id) => {
  const { name } = categories.find((c) => c.id === id)
  return { id, name }
}

const variant = (id, sku, gross, size, isDefault) => ({
  id,
  sku,
  barcode: null,
  ean13: null,
  is_default: isDefault,
  price_gross: gross,
  price_net: gross,
  options: [{ type: "Size", value: size }],
  images: [],
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
    available_stock: "4.000",
    stock_status: "low",
    category: ref(CATEGORY.honey),
    labels: ["Raw", "Unheated", "500 g glass jar"],
    passport: { public_code: LOT.farmed },
    images: [link("honey-1"), link("honey-2")],
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
    stock_status: "sold_out",
    category: ref(CATEGORY.honey),
    labels: [],
    // Points at an unpublished code: the API's 404, so the product page shows no passport card.
    passport: { public_code: LOT.unknown },
    images: [],
    variants: [variant(VARIANT.wax, "WAX-1", 900, "200 g", true)],
  },
  {
    // Sold by size, as on staging: the product row has no price of its own (0), the
    // variants carry them, default first.
    id: PRODUCT.coffee,
    name: "Ruaha Coffee",
    brand: "Ruaha",
    type: "physical",
    short_description: "Arabica from Ilundo, sun-dried on raised beds.",
    description: null,
    ean13: null,
    barcode: null,
    price_gross: 0,
    price_net: 0,
    currency: "USD",
    currency_decimals: 2,
    available_stock: null,
    stock_status: null,
    category: ref(CATEGORY.coffee),
    labels: [],
    passport: null,
    images: [],
    variants: [
      variant(VARIANT.coffee250, "RUA-250", 1200, "250 g", true),
      variant(VARIANT.coffee1k, "RUA-1K", 4000, "1 kg", false),
    ],
  },
]

// AFM's own catalogue: names and prices from africanfarmersmarketonline.com.
let afmId = 0
const afm = (category, name, cents, stock, short, extra = {}) => {
  const n = String(++afmId).padStart(2, "0")
  const id = `afa000${n}-0000-4000-8000-0000000000${n}`
  return {
    id,
    name,
    brand: "African Farmers Market",
    type: "physical",
    short_description: short,
    description: null,
    ean13: null,
    barcode: null,
    price_gross: cents,
    price_net: cents,
    currency: "USD",
    currency_decimals: 2,
    available_stock: stock,
    stock_status:
      stock === null
        ? null
        : Number(stock) === 0
          ? "sold_out"
          : Number(stock) < 5
            ? "low"
            : "in_stock",
    category: ref(category),
    labels: [],
    passport: null,
    images: [link(`afm-${n}`)],
    variants: [
      variant(
        `afb000${n}-0000-4000-8000-0000000000${n}`,
        `AFM-${n}`,
        cents,
        "One size",
        true
      ),
    ],
    ...extra,
  }
}
products.push(
  afm(
    CATEGORY.honey,
    "Mary's Tanzanian Wildflower Honey",
    2799,
    "18.000",
    "Raw, unfiltered wildflower honey from the Illundo beekeepers.",
    {
      labels: ["Raw", "Unfiltered", "13 oz"],
      passport: { public_code: LOT.recalled },
    }
  ),
  afm(
    CATEGORY.coffee,
    "Rift Valley Ground Coffee",
    2999,
    "22.000",
    "100% Arabica from Utengule Farm, ground for the pot."
  ),
  afm(
    CATEGORY.coffee,
    "Africa's Wildest Whole Bean",
    2499,
    "0.000",
    "A peaberry-rich whole bean blend."
  ),
  afm(
    CATEGORY.tea,
    "Mary's Pantry Roselle Tea",
    1999,
    "14.000",
    "Caffeine-free roselle from Tanzania."
  ),
  afm(CATEGORY.tea, "Kizazi Tea", 2999, "9.000", "Kenyan black tea, 18 oz.", {
    passport: { public_code: LOT.quarantine },
  }),
  afm(
    CATEGORY.spices,
    "Whole Cloves",
    1299,
    "30.000",
    "Tanzanian cloves, 4 oz."
  ),
  afm(
    CATEGORY.spices,
    "Green Cardamom",
    1599,
    "3.000",
    "Tanzanian cardamom pods, 4 oz."
  ),
  afm(
    CATEGORY.baskets,
    "Swahili Saffron Orange Basket",
    3800,
    "4.000",
    "Handwoven; each one a little different."
  ),
  afm(
    CATEGORY.baskets,
    "Ruaha Grasslands Natural Basket",
    2000,
    "7.000",
    "Natural grass weave from Tanzania."
  ),
  afm(
    CATEGORY.toys,
    "Handcrafted Tanzanian Elephant",
    1200,
    "11.000",
    "A hand-carved elephant figurine."
  ),
  afm(
    CATEGORY.toys,
    "East African Bao Board",
    2999,
    "5.000",
    "The classic count-and-capture board game."
  ),
  afm(
    CATEGORY.art,
    "Daisies, original by Benard Omondi",
    45000,
    "1.000",
    "Acrylic on canvas, painted in Nairobi."
  ),
  afm(
    CATEGORY.books,
    "The Mary",
    2200,
    "40.000",
    "A picture book about the women who feed East Africa."
  ),
  afm(
    CATEGORY.apparel,
    "Got Asali? T-Shirt",
    2499,
    "12.000",
    "100% cotton, natural colour."
  ),
  afm(
    CATEGORY.stickers,
    "AFM Stickers",
    50,
    "200.000",
    "Four designs: Illundo Farmers, The Mary, Asali, AFM."
  )
)

// Catalogue page 2: one in-stock product with a very long name (24 per page in the API;
// page 1 here is short so the shop's pager can be tried with few fixtures).
const pageTwo = [
  {
    ...products[1],
    category: ref(CATEGORY.honey),
    id: PRODUCT.long,
    name: LONG_NAME,
    short_description: null,
    available_stock: "25.000",
    stock_status: "in_stock",
    passport: null,
    variants: [variant(VARIANT.long, "LONG-1", 3200, "1 kg", true)],
    price_gross: 3200,
    price_net: 3200,
  },
]
const catalogue = [products, pageTwo]
const allProducts = catalogue.flat()

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
  [LOT.quarantine]: () => lot(LOT.quarantine, "quarantine", []),
  [LOT.farmed]: () =>
    lot(LOT.farmed, "released", [
      {
        village: "Ilundo",
        farmers: 12,
        harvest_start: "2026-07-01",
        harvest_end: "2026-07-20",
      },
    ]),
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
  lines,
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
  lines,
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
  const email = body.customer?.email ?? ""
  if (email === BUSY_EMAIL) return [429, { message: "Too many requests." }]
  if (email === CHECKOUT_DOWN_EMAIL) return [503, { message: SECRET_BODY }]
  const errors = {}
  if (!email.includes("@"))
    errors["customer.email"] = ["The email must be a valid email address."]
  let total = 0
  const lines = []
  for (const [i, line] of (body.lines ?? []).entries()) {
    const product = allProducts.find((p) => p.id === line.product_id)
    const variant = product?.variants.find(
      (v) => v.id === line.product_variant_id
    )
    if (!product) errors[`lines.${i}.product_id`] = ["Not found."]
    else if (Number(line.quantity) > STOCK_LIMIT)
      errors[`lines.${i}.quantity`] = [
        `Not enough stock: ${STOCK_LIMIT} available.`,
      ]
    else {
      const unit = (variant ?? product).price_gross
      total += unit * Number(line.quantity)
      lines.push({
        title: [product.name, variant?.options[0]?.value]
          .filter(Boolean)
          .join(" · "),
        product_id: product.id,
        product_variant_id: variant?.id ?? null,
        quantity: Number(line.quantity).toFixed(2),
        unit_price_gross: unit,
        total_gross: unit * Number(line.quantity),
      })
    }
  }
  if (Object.keys(errors).length)
    return [422, { message: "The given data was invalid.", errors }]
  const order = {
    id: randomUUID(),
    number: `WEB-${++numbers}`,
    // `status-<code>@e2e.test` places an order already in that status (label checks).
    status:
      /^status-([a-z_]+)@e2e\.test$/.exec(email)?.[1] ??
      (email === EXPIRED_EMAIL || email === REFUNDED_EMAIL
        ? "cancelled"
        : "awaiting_payment"),
    subtotal: total,
    vat: 0,
    total,
    lines,
    paid_at: email === REFUNDED_EMAIL ? new Date().toISOString() : null,
    expires_at: new Date(Date.now() + 30 * 60_000).toISOString(),
    noProvider: email === NO_PROVIDER_EMAIL,
    providerDown: email === PROVIDER_DOWN_EMAIL,
    latePayment: email === LATE_PAYMENT_EMAIL,
    cancelPayment: email === CANCEL_PAYMENT_EMAIL,
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

// Warm earth tones, picked by name, so AFM's photos read as different products.
const tones = ["#8b8171", "#b98336", "#7a4b2a", "#5f6b3a", "#a4552d", "#3d5a5b"]
const image = (label) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 600"><rect width="1200" height="600" fill="${tones[[...label].reduce((n, c) => n + c.charCodeAt(0), 0) % tones.length]}"/><text x="600" y="300" fill="#fff" font-size="64" text-anchor="middle">${label}</text></svg>`

const handler = async (req, res) => {
  const path = new URL(req.url, MOCK_ORIGIN).pathname
  if (path === "/health") return send(res, 200, { ok: true })
  // Stand-in for the provider's hosted page: pays at once (or, for a cancelling buyer,
  // doesn't) and sends the customer back.
  const paying = orders.get(path.match(/^\/pay\/([^/]+)$/)?.[1])
  if (paying) {
    if (!paying.cancelPayment)
      Object.assign(paying, {
        status: "open",
        paid_at: new Date().toISOString(),
      })
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
  if (orderPath?.[1] === FAILING_ORDER) return send(res, 503, failing)
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
    if (order.providerDown)
      return send(res, 503, { message: "payment provider unavailable" })
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
  if (path === "/v1/storefront/categories")
    return send(res, 200, {
      data: categories.map((c) => ({
        ...c,
        product_count: allProducts.filter((p) => p.category?.id === c.id)
          .length,
      })),
    })
  if (path === "/v1/storefront/products") {
    const category = new URL(req.url, MOCK_ORIGIN).searchParams.get("category")
    if (category) {
      // One page per category here; the API pages it like the full list.
      const data = allProducts.filter((p) => p.category?.id === category)
      return send(res, 200, {
        data,
        meta: {
          current_page: 1,
          from: data.length ? 1 : null,
          to: data.length || null,
          last_page: 1,
          per_page: 24,
          total: data.length,
        },
      })
    }
    // Two pages of products; a later page is empty, as the API answers past the end.
    const page = Number(
      new URL(req.url, MOCK_ORIGIN).searchParams.get("page") ?? 1
    )
    if (page === FAILING_PAGE) return send(res, 503, failing)
    const data = catalogue[page - 1] ?? []
    const before = catalogue.slice(0, page - 1).flat().length
    return send(res, 200, {
      data,
      meta: {
        current_page: page,
        from: data.length ? before + 1 : null,
        to: data.length ? before + data.length : null,
        last_page: catalogue.length,
        per_page: 24,
        total: allProducts.length,
      },
    })
  }
  const product = path.match(/^\/v1\/storefront\/products\/([^/]+)$/)?.[1]
  if (product === PRODUCT.failing) return send(res, 503, failing)
  if (product === PRODUCT.accountsKeyRevoked)
    return send(res, 401, { message: "Unauthenticated." })
  if (product === PRODUCT.accountsKeyNoScope)
    return send(res, 403, { message: "This action is unauthorized." })
  if (product) {
    const found = allProducts.find((p) => p.id === product)
    return found
      ? send(res, 200, { data: found })
      : send(res, 404, { message: "Not found." })
  }
  const code = path.match(/^\/v1\/storefront\/lots\/([^/]+)$/)?.[1]
  if (code === LOT.failing) return send(res, 503, failing)
  if (code && lots[code]) return send(res, 200, { data: lots[code]() })
  send(res, 404, { message: "Not found." })
}

createServer(handler).listen(MOCK_PORT, "127.0.0.1", () =>
  console.log(`e2e mock storefront on ${MOCK_ORIGIN}`)
)
// The file host: images only, like the bucket behind signed links.
createServer((req, res) =>
  new URL(req.url, FILES_ORIGIN).pathname.startsWith("/files/")
    ? handler(req, res)
    : send(res, 404, { message: "Not found." })
).listen(FILES_PORT, "127.0.0.1")
