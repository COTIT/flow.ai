"use client"

import { useQuery } from "@tanstack/react-query"

import { tillSalesQuote } from "@genesy/api-client/endpoints/till"
import type { QuoteRequest } from "@genesy/api-client/model"
import { toSaleLines, type CartLine } from "@/lib/cart-store"
import { useOnline } from "@/lib/use-online"
import { useTill } from "@/lib/till-store"

/**
 * What the API would book for the cart right now (`POST /till/sales/quote`, writes nothing):
 * total, cash rounding, due and change. Offline there is none; the cart shows its estimate.
 */
export function useQuote(
  lines: CartLine[],
  method: "cash" | "card",
  tendered: number | null
) {
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  const online = useOnline()
  const body: QuoteRequest = {
    lines: toSaleLines(lines),
    payment:
      method === "cash"
        ? { method: "cash", ...(tendered != null ? { tendered } : {}) }
        : { method: "card" },
  }
  const query = useQuery({
    queryKey: ["till-quote", workspace, body],
    queryFn: ({ signal }) =>
      tillSalesQuote(workspace, body, undefined, signal).then((r) => r.data),
    enabled: online && lines.length > 0,
    staleTime: 30_000,
  })
  return { online, quote: online ? query.data : undefined, error: query.error }
}
