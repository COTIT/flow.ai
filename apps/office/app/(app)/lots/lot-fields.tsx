"use client"

import { useTranslations } from "next-intl"

import { Input } from "@genesy/ui/components/input"
import { Field, selectClass } from "@/components/field"

// The statuses the API documents for a lot, for the picker and the filter.
export const lotStatuses = ["released", "quarantine", "recalled"] as const

/** The lot fields shared by create and edit; values as the API stores them. */
export function LotFields({
  errors,
  lot,
}: {
  errors: Record<string, string[]>
  lot?: {
    code: string
    status: string
    produced_on: string | null
    best_before: string | null
    expires_on: string | null
    origin: string | null
  }
}) {
  const t = useTranslations()
  return (
    <>
      <Field id="lot-code" label={t("lots.code")} errors={errors.code}>
        <Input name="code" defaultValue={lot?.code} required />
      </Field>
      <Field id="lot-status" label={t("lots.status")} errors={errors.status}>
        <select
          name="status"
          className={selectClass}
          defaultValue={lot?.status ?? "released"}
        >
          {lotStatuses.map((s) => (
            <option key={s} value={s}>
              {t(`lots.statuses.${s}`)}
            </option>
          ))}
        </select>
      </Field>
      <div className="grid gap-3 sm:grid-cols-3">
        <Field
          id="lot-produced"
          label={t("lots.producedOn")}
          errors={errors.produced_on}
        >
          <Input
            name="produced_on"
            type="date"
            defaultValue={lot?.produced_on ?? ""}
          />
        </Field>
        <Field
          id="lot-best-before"
          label={t("lots.bestBefore")}
          errors={errors.best_before}
        >
          <Input
            name="best_before"
            type="date"
            defaultValue={lot?.best_before ?? ""}
          />
        </Field>
        <Field
          id="lot-expires"
          label={t("lots.expiresOn")}
          errors={errors.expires_on}
        >
          <Input
            name="expires_on"
            type="date"
            defaultValue={lot?.expires_on ?? ""}
          />
        </Field>
      </div>
      <Field id="lot-origin" label={t("lots.origin")} errors={errors.origin}>
        <Input name="origin" defaultValue={lot?.origin ?? ""} />
      </Field>
    </>
  )
}
