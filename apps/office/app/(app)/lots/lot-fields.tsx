"use client"

import { useTranslations } from "next-intl"

import { Input } from "@genesy/ui/components/input"
import { Field, selectClass } from "@/components/field"

// The statuses the API documents for a lot, for the list filter.
export const lotStatuses = ["released", "quarantine", "recalled"] as const

/** The lot fields shared by create and edit, flat so each caller lays them out; values as the API stores them. */
export function LotFields({
  errors,
  lot,
}: {
  errors: Record<string, string[]>
  lot?: {
    code: string
    status: string
    next_statuses: string[]
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
        <Input
          name="code"
          defaultValue={lot?.code}
          className="font-mono"
          required
        />
      </Field>
      <Field id="lot-origin" label={t("lots.origin")} errors={errors.origin}>
        <Input name="origin" defaultValue={lot?.origin ?? ""} />
      </Field>
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
      {lot ? (
        // Only the moves the API allows next; a recalled lot has none.
        <Field id="lot-status" label={t("lots.status")} errors={errors.status}>
          <select
            name="status"
            className={selectClass}
            defaultValue={lot.status}
            disabled={lot.next_statuses.length === 0}
          >
            {[lot.status, ...lot.next_statuses].map((s) => (
              <option key={s} value={s}>
                {t(`lots.statuses.${s}`)}
              </option>
            ))}
          </select>
        </Field>
      ) : null}
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
    </>
  )
}
