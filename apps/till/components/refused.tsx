"use client"

import { useTranslations } from "next-intl"

import { Notice } from "@verzo/ui/components/notice"
import { inWords, refusal } from "@/lib/refusal"

/** The API's per-field messages of a 422, flat. */
const fieldErrors = (e: unknown) =>
  Object.values(
    (e as { response?: { data?: { errors?: Record<string, string[]> } } })
      ?.response?.data?.errors ?? {}
  ).flat()

/** A refused call as a Notice: the cashier's words first, the API's sentence and fields below. */
export function Refused({
  error,
  fallback,
}: {
  error: unknown
  fallback: string
}) {
  const tr = useTranslations("refusals")
  const r = refusal(error, tr, fallback)
  const fields = fieldErrors(error).map((m) => inWords(m, tr))
  return (
    <Notice variant="error" title={r.title}>
      {r.detail && !fields.includes(r.detail) ? <p>{r.detail}</p> : null}
      {fields.map((m) => (
        <p key={m}>{m}</p>
      ))}
    </Notice>
  )
}
