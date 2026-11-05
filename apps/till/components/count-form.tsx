"use client"

import * as React from "react"
import { ChevronRight, Minus, Plus, X } from "lucide-react"
import { useTranslations } from "next-intl"

import type { CountLine } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { cn } from "@genesy/ui/lib/utils"
import { Keypad, type PadKey } from "@/components/keypad"
import { useCountQuote } from "@/lib/use-count-quote"
import { useTill } from "@/lib/till-store"
import { useTillMoney } from "@/lib/use-money"

/** `fixed`: a note or coin the register accepts (minor units); otherwise a typed value. */
type Row = { fixed?: number; denomination: string; count: string }
const empty: Row = { denomination: "", count: "" }

/**
 * A cash count (TillDrawer): one row per note or coin the register accepts (stepper, a count
 * field the pad types into), and on the right the figures, the pad and the submit button.
 * Typed extra rows only when the register lists no notes.
 */
export function CountForm({
  title,
  sub,
  badge,
  expected,
  aside,
  submitLabel,
  pending,
  onSubmit,
}: {
  title: string
  sub: React.ReactNode
  badge: React.ReactNode
  /** The API's expected cash, when a shift is open. */
  expected?: string
  /** Notices for the right column (above the pad). */
  aside?: React.ReactNode
  submitLabel: string
  pending: boolean
  onSubmit: (count: CountLine[]) => void
}) {
  const t = useTranslations("count")
  const { money } = useTillMoney()
  const accepted = useTill((s) => s.session?.register.accepted_cash) ?? []
  const [rows, setRows] = React.useState<Row[]>(() =>
    accepted.length
      ? // Largest note first, as the drawer is counted (TillDrawer).
        [...accepted]
          .sort((a, b) => b - a)
          .map((v) => ({ fixed: v, denomination: "", count: "" }))
      : [empty]
  )
  const [selected, setSelected] = React.useState(0)
  const [invalid, setInvalid] = React.useState(false)

  // A listed note goes as its minor units, a typed one as the text typed: the API converts it.
  const sent = rows.filter(
    (r) => r.count.trim() || (!r.fixed && r.denomination.trim())
  )
  const lines = sent.map((r) => ({
    ...(r.fixed
      ? { denomination: r.fixed }
      : { value: r.denomination.trim() || null }),
    count: /^\d+$/.test(r.count.trim()) ? Number(r.count) : null,
  }))
  const valid = lines.every(
    (l) => ("denomination" in l || l.value != null) && l.count != null
  )
  // The API adds the count up and compares it with the drawer; the till only shows it.
  const { online, quote } = useCountQuote(valid ? (lines as CountLine[]) : null)
  /** The API's denomination × pieces for a row; null until it has answered. */
  const rowTotal = (r: Row) => {
    const i = sent.indexOf(r)
    return i < 0 ? null : (quote?.lines[i] ?? null)
  }

  const set = (i: number, patch: Partial<Row>) =>
    setRows((all) => all.map((r, j) => (j === i ? { ...r, ...patch } : r)))
  const pieces = (r: Row) =>
    /^\d+$/.test(r.count.trim()) ? Number(r.count) : 0
  const press = (k: PadKey) => {
    const r = rows[selected]
    if (!r) return
    set(selected, {
      count:
        k === "back"
          ? r.count.slice(0, -1)
          : k === "clear"
            ? ""
            : (r.count + k).replace(/^0+(?=\d)/, "").slice(0, 5),
    })
  }
  const label = (r: Row, i: number) =>
    r.fixed ? money(r.fixed) : r.denomination || t("row", { n: i + 1 })

  return (
    <form
      className="split split-wide"
      onSubmit={(e) => {
        e.preventDefault()
        if (!valid) return setInvalid(true)
        setInvalid(false)
        onSubmit(lines as CountLine[])
      }}
    >
      <section
        className="till-card gap-1 overflow-y-auto"
        aria-labelledby="count-title"
      >
        <div className="mb-1.5 flex items-center justify-between gap-3">
          <div>
            <h1 id="count-title" className="card-title text-lg">
              {title}
            </h1>
            <p className="text-[13px] text-muted-foreground">{sub}</p>
          </div>
          {badge}
        </div>
        {rows.map((row, i) => (
          <div
            key={i}
            className={cn("count-row", i === selected && "till-tile")}
          >
            {row.fixed ? (
              <span className="text-[17px] font-semibold tabular-nums">
                {money(row.fixed)}
              </span>
            ) : (
              <Input
                aria-label={t("denomination", { n: i + 1 })}
                inputMode="decimal"
                placeholder={t("value")}
                className="h-11"
                value={row.denomination}
                onChange={(e) => set(i, { denomination: e.target.value })}
              />
            )}
            <span className="qstep">
              <button
                type="button"
                aria-label={t("less", { value: label(row, i) })}
                onClick={() => {
                  setSelected(i)
                  set(i, { count: String(Math.max(0, pieces(row) - 1)) })
                }}
              >
                <Minus aria-hidden className="size-[18px]" />
              </button>
              <input
                aria-label={
                  row.fixed
                    ? t("piecesOf", { value: money(row.fixed) })
                    : t("pieces", { n: i + 1 })
                }
                // The pad types here; no on-screen keyboard over the pad.
                inputMode="none"
                autoComplete="off"
                placeholder="0"
                className="count"
                value={row.count}
                onFocus={() => setSelected(i)}
                onChange={(e) => set(i, { count: e.target.value })}
              />
              <button
                type="button"
                aria-label={t("more", { value: label(row, i) })}
                onClick={() => {
                  setSelected(i)
                  set(i, { count: String(pieces(row) + 1) })
                }}
              >
                <Plus aria-hidden className="size-[18px]" />
              </button>
            </span>
            <span className="flex items-center justify-end gap-1 text-right font-semibold tabular-nums">
              {rowTotal(row) == null ? null : money(rowTotal(row))}
              {row.fixed ? null : (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="btn-44"
                  aria-label={t("remove", { n: i + 1 })}
                  disabled={rows.length === 1}
                  onClick={() => {
                    setSelected(0)
                    setRows(rows.filter((_, j) => j !== i))
                  }}
                >
                  <X />
                </Button>
              )}
            </span>
          </div>
        ))}
        {accepted.length ? null : (
          <Button
            type="button"
            variant="outline"
            className="btn-44 mt-2 w-fit"
            onClick={() => setRows([...rows, empty])}
          >
            <Plus /> {t("add")}
          </Button>
        )}
      </section>

      <section className="flex min-h-0 flex-col gap-3">
        <div className="till-card strong tight">
          {expected ? (
            <div className="flex items-center justify-between gap-3">
              <span id="expected-label" className="text-muted-foreground">
                {t("expected")}
              </span>
              <output
                aria-labelledby="expected-label"
                className="font-semibold tabular-nums"
              >
                {expected}
              </output>
            </div>
          ) : null}
          <div className="flex items-center justify-between gap-3">
            <span id="counted-label" className="font-semibold">
              {t("total")}
            </span>
            <output aria-labelledby="counted-label" className="grand">
              {money(quote?.total)}
            </output>
          </div>
          {quote?.difference != null ? (
            <div className="flex items-center justify-between gap-3">
              <span id="difference-label" className="text-muted-foreground">
                {t("difference")}
              </span>
              <output
                aria-labelledby="difference-label"
                className="font-semibold tabular-nums"
              >
                {money(quote.difference)}
              </output>
            </div>
          ) : null}
        </div>
        {online ? null : <Notice variant="info" title={t("offlineTotal")} />}
        {aside}
        {invalid ? <Notice variant="error" title={t("invalid")} /> : null}
        <Keypad fn="clear" onKey={press} />
        <p className="text-xs text-muted-foreground">
          {t("padHint", { row: label(rows[selected] ?? empty, selected) })}
        </p>
        <button type="submit" className="charge mt-auto" disabled={pending}>
          <span>{submitLabel}</span>
          <ChevronRight aria-hidden className="size-[22px]" />
        </button>
      </section>
    </form>
  )
}
