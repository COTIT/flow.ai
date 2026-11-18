"use client"

import { useTranslations } from "next-intl"

/** A label for an API word under `taxes.<group>`, or the word itself when unknown. */
export function useWord() {
  const t = useTranslations("taxes")
  return (group: string, word: string) =>
    t.has(`${group}.${word}`) ? t(`${group}.${word}`) : word
}

/** Every refusal code the tax profile API sends, as the sentence staff read (never the code). */
export function useReasons() {
  const t = useTranslations("taxes.reasons")
  return Object.fromEntries(
    [
      "stale_document",
      "tax_profile_verified",
      "tax_profile_correction_exists",
      "tax_profile_overlap",
    ].map((c) => [c, t(c)])
  )
}

/** "Tanzania" for "TZ" (the browser's names); a subdivision code follows in brackets. */
export const placeName = (
  p: { country: string; jurisdiction_scope?: string | null },
  locale: string
) => {
  let country = p.country
  try {
    country =
      new Intl.DisplayNames([locale], { type: "region" }).of(p.country) ??
      p.country
  } catch {
    // An unknown code stays as typed.
  }
  return p.jurisdiction_scope ? `${country} (${p.jurisdiction_scope})` : country
}
