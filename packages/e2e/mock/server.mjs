// Mock of the Genesy storefront API for e2e runs of server-rendered apps (the shop).
// No dependencies; listens on 127.0.0.1 only. Signed file links point back at this server.
import console from "node:console"
import { createServer } from "node:http"
import { URL } from "node:url"

import {
  APP_KEY,
  LOT,
  MOCK_ORIGIN,
  MOCK_PORT,
  PRODUCT,
  SECRET_BODY,
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
      variant("v1", "ILU-250", 1499, "250 g", true),
      variant("v2", "ILU-500", 2499, "500 g", false),
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
    variants: [variant("v3", "WAX-1", 900, "200 g", true)],
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
    lot(LOT.recalled, "recalled", [
      {
        first_name: "Asha",
        village: "Ilundo",
        photo: link("asha"),
        harvest_dates: ["2026-07-10"],
      },
    ]),
}

const send = (res, status, body, type = "application/json") => {
  res.writeHead(status, { "content-type": type })
  res.end(type === "application/json" ? JSON.stringify(body) : body)
}

const image = (label) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 600"><rect width="1200" height="600" fill="#8b8171"/><text x="600" y="300" fill="#fff" font-size="64" text-anchor="middle">${label}</text></svg>`

createServer((req, res) => {
  const path = new URL(req.url, MOCK_ORIGIN).pathname
  if (path === "/health") return send(res, 200, { ok: true })
  if (path.startsWith("/files/"))
    return send(res, 200, image(path.slice(7)), "image/svg+xml")
  if (!path.startsWith("/v1/storefront/"))
    return send(res, 404, { message: "Not found." })
  if (req.headers.authorization !== `Bearer ${APP_KEY}`)
    return send(res, 401, { message: "Unauthenticated." })

  const failing = { message: SECRET_BODY }
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
