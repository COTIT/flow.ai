/**
 * Reference data is translated in the DATABASE — QA #31.
 *
 * Crop, equipment and category names live in `name_en` / `name_sw` because
 * those rows are created at runtime and a repo file cannot translate them
 * (CLAUDE.md, "Layout"). Every query that needs a name asks for BOTH columns,
 * and the choice between them happens here, at render.
 *
 * **Never choose inside a `queryFn`.** The result would be cached under a key
 * about the row, not about the language, so whichever locale resolved first
 * would be served from then on — which is exactly how the Ilundo officer came
 * to see `Kahawa` in the verify queue and `Maize` on a cycle detail in the
 * same session. `src/features/localisation.test.ts` enforces this.
 */

export interface LocalisedNames {
  name_en: string | null
  name_sw: string | null
}

/**
 * The name in the active language, falling back to English.
 *
 * A missing Swahili name falls back rather than rendering blank: an untranslated
 * row is a gap in the data, and English is the honest thing to show for it.
 */
export function localisedName(
  row: LocalisedNames | null | undefined,
  language: string | undefined,
): string {
  if (!row) return ''
  // i18next reports a resolved language, which can carry a region: `sw-TZ` is
  // still Swahili.
  const swahili = language?.startsWith('sw') ?? false
  return (swahili ? (row.name_sw ?? '') : '') || (row.name_en ?? '')
}

/**
 * The same choice for authored text that lives in `<field>_en` / `<field>_sw`
 * columns — survey titles, questions and options. Admin writes the English;
 * Swahili is filled in once a native reviewer has supplied it.
 */
export function localisedField<F extends string>(
  row: Partial<Record<`${F}_en` | `${F}_sw`, string | null>> | null | undefined,
  field: F,
  language: string | undefined,
): string {
  if (!row) return ''
  const swahili = language?.startsWith('sw') ?? false
  const sw = row[`${field}_sw` as `${F}_sw`] ?? ''
  const en = row[`${field}_en` as `${F}_en`] ?? ''
  return (swahili ? sw : '') || en
}
