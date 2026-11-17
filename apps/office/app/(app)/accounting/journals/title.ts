/** "Journal 12" once posted; a draft has no number yet. */
export const journalTitle = (
  t: (key: string, values?: Record<string, string | number>) => string,
  j: { number?: number | null }
) =>
  j.number == null
    ? t("accounting.journals.draftTitle")
    : t("accounting.journals.numbered", { number: j.number })
