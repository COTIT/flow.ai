"use client"

import * as React from "react"
import { Plus, X } from "lucide-react"
import { useTranslations } from "next-intl"

import type { CountLine } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { toMinor } from "@genesy/ui/lib/money"
import { useTill } from "@/lib/till-store"
import { useTillMoney } from "@/lib/use-money"

/** `fixed`: a note or coin the register accepts (minor units); otherwise a typed value. */
type Row = { fixed?: number; denomination: string; count: string }
const empty: Row = { denomination: "", count: "" }

/** A cash count: one row per note or coin the register accepts, plus any typed extra rows. */
export function CountForm({
  submitLabel,
  pending,
  onSubmit,
}: {
  submitLabel: string
  pending: boolean
  onSubmit: (count: CountLine[]) => void
}) {
  const t = useTranslations("count")
  const { money, decimals } = useTillMoney()
  const accepted = useTill((s) => s.session?.register.accepted_cash) ?? []
  const [rows, setRows] = React.useState<Row[]>(() =>
    accepted.length
      ? accepted.map((v) => ({ fixed: v, denomination: "", count: "" }))
      : [empty]
  )
  const [invalid, setInvalid] = React.useState(false)

  const lines = rows
    .filter((r) => r.count.trim() || (!r.fixed && r.denomination.trim()))
    .map((r) => ({
      denomination:
        r.fixed ??
        (decimals == null ? null : toMinor(r.denomination, decimals)),
      count: /^\d+$/.test(r.count.trim()) ? Number(r.count) : null,
    }))
  const valid = lines.every((l) => l.denomination != null && l.count != null)
  // Counting the notes, not pricing: the API's own figures come back after submit.
  const total = valid
    ? lines.reduce(
        (sum, l) => sum + BigInt(l.denomination!) * BigInt(l.count!),
        0n
      )
    : null

  const set = (i: number, patch: Partial<Row>) =>
    setRows(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)))

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault()
        if (!valid) return setInvalid(true)
        setInvalid(false)
        onSubmit(lines as CountLine[])
      }}
    >
      <ul className="flex flex-col gap-2">
        {rows.map((row, i) => (
          <li key={i} className="flex items-center gap-2">
            {row.fixed ? (
              <span className="flex-1 tabular-nums">{money(row.fixed)}</span>
            ) : (
              <Input
                aria-label={t("denomination", { n: i + 1 })}
                inputMode="decimal"
                placeholder={t("value")}
                value={row.denomination}
                onChange={(e) => set(i, { denomination: e.target.value })}
              />
            )}
            <span aria-hidden>×</span>
            <Input
              aria-label={
                row.fixed
                  ? t("piecesOf", { value: money(row.fixed) })
                  : t("pieces", { n: i + 1 })
              }
              inputMode="numeric"
              placeholder={t("count")}
              className="w-24"
              value={row.count}
              onChange={(e) => set(i, { count: e.target.value })}
            />
            {row.fixed ? null : (
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label={t("remove", { n: i + 1 })}
                disabled={rows.length === 1}
                onClick={() => setRows(rows.filter((_, j) => j !== i))}
              >
                <X />
              </Button>
            )}
          </li>
        ))}
      </ul>
      <Button
        type="button"
        variant="outline"
        className="w-fit"
        onClick={() => setRows([...rows, empty])}
      >
        <Plus /> {t("add")}
      </Button>
      <p className="text-sm">
        {t("total")}:{" "}
        <span className="font-medium tabular-nums">
          {total == null ? "—" : money(Number(total))}
        </span>
      </p>
      {invalid ? (
        <p role="alert" className="text-sm text-destructive">
          {t("invalid")}
        </p>
      ) : null}
      <Button type="submit" disabled={pending} className="w-fit">
        {submitLabel}
      </Button>
    </form>
  )
}
