const prefixes: Record<string, string> = {
  en: "Confirm action",
  nl: "Bevestig actie",
  sw: "Thibitisha kitendo",
}

/** This names one current proposal; it does not prove the user heard a readback. */
export function confirmationPhrase(
  code: string,
  locale: string,
  kind: "action" | "batch" = "action"
): string | null {
  if (!Object.hasOwn(prefixes, locale)) return null
  const prefix =
    kind === "batch"
      ? (
          {
            en: "Confirm batch",
            nl: "Bevestig reeks",
            sw: "Thibitisha kundi",
          } as Record<string, string>
        )[locale]
      : prefixes[locale]
  return prefix && /^[0-9]{6}$/.test(code) ? `${prefix} ${code}` : null
}

/** Only case and whitespace vary. Negation, extra words and spoken digit words fail closed. */
export function matchesSpokenConfirmation(
  text: string,
  code: string,
  locale: string,
  kind: "action" | "batch" = "action"
): boolean {
  const phrase = confirmationPhrase(code, locale, kind)
  if (!phrase) return false
  return (
    text.trim().replace(/\s+/gu, " ").toLocaleLowerCase(locale) ===
    phrase.toLocaleLowerCase(locale)
  )
}
