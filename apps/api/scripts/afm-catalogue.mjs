#!/usr/bin/env node
// Dev tool, not a runtime route: reads African Farmers Market's public Squarespace store (AFM
// agreed to its use on staging) and writes the demo seed's catalogue, src/demo/afm_catalogue.json.
// Polite: one request at a time, a pause between them, a named user agent.
//   node apps/api/scripts/afm-catalogue.mjs            # fetch and write
// Photos are not downloaded here: the seed fetches them (`seed-demo --images`) into the bucket.
import { writeFile } from "node:fs/promises"
import { fileURLToPath } from "node:url"

const SITE = "https://www.africanfarmersmarketonline.com"
const AGENT =
  "GenesyCatalogImport/1.0 (+https://genesy.app; AFM staging demo, with consent)"
const PAUSE_MS = 1500
const OUT = fileURLToPath(
  new URL("../src/demo/afm_catalogue.json", import.meta.url)
)

// AFM's store categories → the shop's (as AFM's own category pages group them).
export const CATEGORIES = {
  "Tanzanian Honey": "Honey",
  Coffee: "Coffee",
  Tea: "Tea",
  Spices: "Spices",
  Baskets: "Baskets",
  Bags: "Baskets",
  Toys: "Toys & games",
  "3D Figurines": "Toys & games",
  Artwork: "Art",
  Books: "Books",
  Apparel: "Apparel",
  Stickers: "Stickers",
}

const SHORT_MAX = 255
const LONG_MAX = 1500

const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " }

/** Squarespace rich text → plain paragraphs joined by a blank line. */
export function htmlToText(html) {
  return (html ?? "")
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/(p|h[1-6]|li|div)>/gi, "\n\n")
    .replace(/<[^>]*>/g, "")
    .replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e) => {
      if (e[0] !== "#") return ENTITIES[e.toLowerCase()] ?? m
      const n =
        e[1].toLowerCase() === "x"
          ? parseInt(e.slice(2), 16)
          : parseInt(e.slice(1), 10)
      return String.fromCodePoint(n)
    })
    .split(/\n\s*\n/)
    .map((p) =>
      p
        .replace(/[ \t ]+/g, " ")
        .replace(/ *\n */g, "\n")
        .trim()
    )
    .filter(Boolean)
    .join("\n\n")
}

/** `text` cut to `max` characters at the last sentence end, else word, that fits. */
export function cut(text, max) {
  if (text.length <= max) return text
  const head = text.slice(0, max)
  const sentence = Math.max(
    head.lastIndexOf(". "),
    head.lastIndexOf("! "),
    head.lastIndexOf("? ")
  )
  if (sentence > max / 2) return head.slice(0, sentence + 1)
  const paragraph = head.lastIndexOf("\n\n")
  if (paragraph > max / 2) return head.slice(0, paragraph)
  return head.slice(0, head.lastIndexOf(" ")).trimEnd() + "…"
}

/** One store page's JSON → catalogue products. Throws on a category with no mapping. */
export function transform(page) {
  const order = page.nestedCategories.categories.map((c) => c.id)
  const names = Object.fromEntries(
    page.nestedCategories.categories.map((c) => [c.id, c.displayName])
  )
  return page.items.map((item) => {
    const s = item.structuredContent
    const main = [...item.categoryIds].sort(
      (a, b) => order.indexOf(a) - order.indexOf(b)
    )[0]
    const category = CATEGORIES[names[main]]
    if (!category)
      throw new Error(`${item.title}: no shop category for "${names[main]}"`)
    const text = htmlToText(item.excerpt)
    const option = s.variantOptionOrdering[0] ?? null
    return {
      slug: item.urlId,
      title: item.title.replace(/^"(.*)"$/, "$1").trim(),
      category,
      short_description: cut(text.split("\n\n")[0] ?? "", SHORT_MAX),
      description: cut(text, LONG_MAX),
      option,
      variants: s.variants.map((v) => ({
        value: option
          ? (v.optionValues.find((o) => o.optionName === option)?.value ?? null)
          : null,
        sku: v.sku,
        cents: v.price,
      })),
      images: (item.items ?? []).map((i) => `${i.assetUrl}?format=1000w`),
      source_url: `${SITE}${item.fullUrl}`,
    }
  })
}

async function get(url) {
  const res = await fetch(url, {
    headers: { "user-agent": AGENT, accept: "application/json" },
  })
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`)
  return res.json()
}

async function main() {
  const products = []
  let url = `${SITE}/store?format=json`
  while (url) {
    const page = await get(url)
    products.push(...transform(page))
    const next = page.pagination?.nextPage ? page.pagination.nextPageUrl : null
    url = next
      ? `${SITE}${next}${next.includes("?") ? "&" : "?"}format=json`
      : null
    if (url) await new Promise((r) => setTimeout(r, PAUSE_MS))
  }
  products.sort((a, b) => a.slug.localeCompare(b.slug))
  const out = {
    source: SITE,
    note: "AFM's public store, used on staging with AFM's consent. Regular prices in USD cents. Regenerate: node apps/api/scripts/afm-catalogue.mjs",
    products,
  }
  await writeFile(OUT, JSON.stringify(out, null, 2) + "\n")
  const variants = products.reduce((n, p) => n + p.variants.length, 0)
  const images = products.reduce((n, p) => n + p.images.length, 0)
  console.log(
    `${products.length} products, ${variants} variants, ${images} photos → ${OUT}`
  )
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main()
