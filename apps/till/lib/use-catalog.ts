"use client"

import { useQuery } from "@tanstack/react-query"
import { createJSONStorage, persist } from "zustand/middleware"
import { createStore } from "zustand/vanilla"
import { useStore } from "zustand"

import { tillCatalog } from "@genesy/api-client/endpoints/till"
import type { TillProduct } from "@genesy/api-client/model"
import { useTill } from "@/lib/till-store"

/** One thing the till can sell: a product, or one variant of it. */
export type Sellable = {
  productId: string
  variantId: string | null
  name: string
  /** Minor units excl VAT: what the sale books as `unit_price`. */
  unitNet: number
  /** One unit incl VAT as the API priced it: display and the offline estimate only. */
  unitGross: number | null
  codes: string[]
}

type Catalog = {
  /** The register this copy belongs to; another register starts over. */
  registerId: string | null
  products: Record<string, TillProduct>
  /** The API's `next_since` from the last sync; null = next sync is a full read. */
  since: string | null
}

/** The register's catalog, kept on the device so the till can sell offline. */
export const catalogStore = createStore<Catalog>()(
  persist(() => ({ registerId: null, products: {}, since: null }) as Catalog, {
    name: "genesy.till.catalog",
    storage: createJSONStorage(() => localStorage),
    skipHydration: true,
  })
)

async function readAll(workspace: string, updatedSince?: string) {
  const rows: TillProduct[] = []
  let since: string | null = null
  for (let page = 1; ; page++) {
    const res = await tillCatalog(workspace, {
      page,
      page_size: 100,
      ...(updatedSince ? { updated_since: updatedSince } : {}),
    })
    rows.push(...res.data)
    // The first page's mark is the earliest: later pages only add repeats on the next delta.
    since ??= res.next_since
    if (page >= res.meta.last_page) return { rows, since }
  }
}

/** Full read the first time, then only what changed; a row no longer sellable is dropped. */
export async function syncCatalog(workspace: string, registerId: string) {
  const own = catalogStore.getState().registerId === registerId
  const before = own ? catalogStore.getState() : null
  const { rows, since } = await readAll(workspace, before?.since ?? undefined)
  const products = { ...(before?.products ?? {}) }
  for (const p of rows) {
    if (p.sellable) products[p.id] = p
    else delete products[p.id]
  }
  catalogStore.setState({ registerId, products, since })
}

function sellables(products: TillProduct[]): Sellable[] {
  return products.flatMap<Sellable>((p) =>
    p.variants.length
      ? p.variants.map((v) => ({
          productId: p.id,
          variantId: v.id,
          name: [p.name, ...v.options.map((o) => o.value)].join(" · "),
          unitNet: v.unit_price,
          unitGross: v.unit_price_gross,
          codes: [v.ean13, v.barcode, v.sku].filter((c): c is string => !!c),
        }))
      : [
          {
            productId: p.id,
            variantId: null,
            name: p.name,
            unitNet: p.unit_price,
            unitGross: p.unit_price_gross,
            codes: [p.ean13, p.barcode].filter((c): c is string => !!c),
          },
        ]
  )
}

/** The product or variant carrying exactly this code (any case). */
function match(items: Sellable[], code: string) {
  const c = code.trim().toLowerCase()
  return items.find((i) => i.codes.some((x) => x.toLowerCase() === c)) ?? null
}

/**
 * The till's catalog, in one place: `GET /till/catalog` with the till session, synced into a
 * device copy (full, then deltas every minute) that also serves search and offline scans.
 */
export function useTillCatalog() {
  const workspace = useTill((s) => s.device?.workspaceId)
  const registerId = useTill((s) => s.session?.register.id)
  const query = useQuery({
    queryKey: ["till-catalog", registerId],
    queryFn: async () => {
      await syncCatalog(workspace!, registerId!)
      return true
    },
    enabled: Boolean(workspace && registerId),
    refetchInterval: 60_000,
  })
  const copy = useStore(catalogStore)
  const items = sellables(
    copy.registerId === registerId ? Object.values(copy.products) : []
  )
  return {
    error: query.error,
    /** First sync still running and nothing on the device yet. */
    loading: query.isPending && items.length === 0,
    /** Synced, and the register sells nothing. */
    empty: query.isSuccess && items.length === 0,
    /** Products on the device (the copy works offline too). */
    count: items.length,
    /** Everything the device copy can sell, for the tiles. */
    items,
    /** Scanned or typed code: the API's exact EAN/barcode match, else the device copy (offline). */
    lookup: async (code: string): Promise<Sellable | null> => {
      if (!navigator.onLine) return match(items, code)
      try {
        const res = await tillCatalog(workspace!, { code: code.trim() })
        const found = sellables(res.data.filter((p) => p.sellable))
        return match(found, code) ?? found[0] ?? null
      } catch {
        return match(items, code)
      }
    },
    /** Name or code containing the text, case-insensitive; display filter only. */
    search: (text: string) => {
      const q = text.trim().toLowerCase()
      return q
        ? items.filter(
            (i) =>
              i.name.toLowerCase().includes(q) ||
              i.codes.some((c) => c.toLowerCase().includes(q))
          )
        : []
    },
  }
}
