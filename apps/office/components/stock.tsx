"use client"

import Link from "next/link"
import { useTranslations } from "next-intl"

/** A stock row's batch as the API sends it (`lot`: `{id, code, expires_on}` or null). */
export type StockLot = { id: string; code: string } | null

export const lotOf = (lot: unknown) => (lot ?? null) as StockLot

/** A link inside running text that is still a 44 px target on a phone. */
export const inlineLink =
  "font-medium text-foreground underline underline-offset-4 max-md:inline-flex max-md:min-h-11 max-md:items-center"

/** The batch a stock row belongs to, linked to its batch page. */
export function BatchLink({ lot }: { lot: { id: string; code: string } }) {
  const t = useTranslations()
  return (
    <Link className={inlineLink} href={`/lots/${lot.id}`}>
      {t("stock.batch", { code: lot.code })}
    </Link>
  )
}

/** Why a place is below zero, in plain words, with the way to fix it. */
export function BelowZeroWhy({ children }: { children: React.ReactNode }) {
  const t = useTranslations()
  return (
    <p className="mt-1 text-[13px] text-muted-foreground">
      {t("stock.belowZeroWhy")} {children}
    </p>
  )
}

type Row = { id: string; warehouse: unknown; bin: unknown; lot: unknown }

/** A place (warehouse) and its stock rows, in the API's order. `base` is its row without a
 * batch or bin: the one Change stock writes and the one that holds the place's minimum. */
export function byPlace<R extends Row>(rows: R[]) {
  const places: { id: string; name: string; rows: R[]; base?: R }[] = []
  for (const r of rows) {
    const w = (r.warehouse ?? {}) as { id?: string; name?: string }
    let p = places.find((p) => p.id === w.id)
    if (!p) places.push((p = { id: w.id ?? "", name: w.name ?? "", rows: [] }))
    p.rows.push(r)
    if (!r.lot && !r.bin) p.base = r
  }
  return places
}
