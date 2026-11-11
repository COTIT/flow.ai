"use client"

import { useId, useState } from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { useVoice } from "@genesy/voice/components/provider"
import { clarifyOf } from "@genesy/voice/lib/task"
import { flowStore } from "@/lib/flow-store"

const TYPES = ["choose", "set", "add", "remove"] as const
type Kind = (typeof TYPES)[number]

/**
 * Genesy asked which product: pick it from the answer's own records, and say how stock should
 * change. Set total maps to setting levels, Add/Remove to a stock change; Genesy still proposes
 * the exact change for review. Only words and the record id leave this card.
 */
export function Clarify() {
  const t = useTranslations("task.clarify")
  const voice = useVoice()
  const choice = clarifyOf(voice.messages)
  const [picked, setPicked] = useState<string | null>(null)
  const [kind, setKind] = useState<Kind>("choose")
  const [quantity, setQuantity] = useState("")
  const id = useId()
  if (!choice || !voice.connected) return null
  const row = choice.rows.find((r) => r.id === picked)
  const amount = Number(quantity)
  const valid =
    Boolean(row) &&
    (kind === "choose" ||
      (/^\d{1,7}$/.test(quantity) && (kind === "set" || amount > 0)))
  return (
    <form
      key={choice.resultId}
      aria-labelledby="flow-clarify-title"
      className="flex min-w-0 flex-col gap-3 rounded-2xl border border-(--gold) p-4"
      onSubmit={(e) => {
        e.preventDefault()
        if (!row || !valid) return
        const product = row.detail ? `${row.label} (${row.detail})` : row.label
        flowStore.getState().setSelected({ type: "product", ids: [row.id] })
        void voice.send(t(`say.${kind}`, { product, quantity: amount }))
      }}
    >
      <fieldset className="flex min-w-0 flex-col gap-2">
        <legend id="flow-clarify-title" className="mb-2 text-sm font-semibold uppercase">
          {t("title")}
        </legend>
        {choice.rows.map((r) => (
          <label
            key={r.id}
            className="flex min-h-11 cursor-pointer items-center gap-3 rounded-xl border p-2 has-checked:border-(--gold)"
          >
            <input
              type="radio"
              name="flow-clarify-product"
              value={r.id}
              checked={picked === r.id}
              onChange={() => setPicked(r.id)}
            />
            <span className="min-w-0 flex-1 break-words">{r.label}</span>
            {r.detail ? (
              <span className="text-xs text-muted-foreground">{r.detail}</span>
            ) : null}
          </label>
        ))}
      </fieldset>
      <div className="flex min-w-0 flex-wrap items-end gap-3">
        <div className="flex min-w-0 flex-col gap-1 text-sm">
          {/* Named by its own label only, never by its options' text. */}
          <label htmlFor={`${id}-type`}>{t("type")}</label>
          <select
            id={`${id}-type`}
            value={kind}
            onChange={(e) => setKind(e.target.value as Kind)}
            className="min-h-11 rounded-md border bg-background px-2"
          >
            {TYPES.map((type) => (
              <option key={type} value={type}>
                {t(`types.${type}`)}
              </option>
            ))}
          </select>
        </div>
        {kind !== "choose" ? (
          <div className="flex min-w-0 flex-col gap-1 text-sm">
            <label htmlFor={`${id}-quantity`}>{t("quantity")}</label>
            <Input
              id={`${id}-quantity`}
              inputMode="numeric"
              pattern="\d*"
              aria-describedby={`${id}-hint`}
              value={quantity}
              onChange={(e) => setQuantity(e.target.value.trim())}
              className="min-h-11 w-32"
            />
            <span id={`${id}-hint`} className="text-xs text-muted-foreground">
              {t("quantityHint")}
            </span>
          </div>
        ) : null}
        <Button type="submit" className="ml-auto min-h-11" disabled={!valid || voice.busy}>
          {kind === "choose" ? t("continue") : t("prepare")}
        </Button>
      </div>
    </form>
  )
}
