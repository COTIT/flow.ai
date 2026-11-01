import i18next from 'i18next'
import { initReactI18next } from 'react-i18next'

import enCommon from './en/common.json'
import swCommon from './sw/common.json'

// Only `common` exists. Per-feature namespaces are added with their features.
//
// CLAUDE.md: Farmer and Officer surfaces ship complete Swahili; Ops and Tower
// may ship English for the demo. The Swahili is a DRAFT, approved by the
// product owner on 29 Sep 2026: written from docs/i18n-glossary.md, checked by
// blind back-translation, and NOT seen by a native reader. A string is
// "reviewed" only when sw/reviewed.json names who reviewed it. Do not add
// Swahili that is not in the glossary's terms without flagging it in
// sw/flags.json.
//
// Reference data (crop, equipment, equipment_category names) is translated in
// the database via name_en / name_sw, never here: those rows are created at
// runtime and a repo file cannot translate them.
export const supportedLanguages = ['en', 'sw'] as const
export type SupportedLanguage = (typeof supportedLanguages)[number]

/** app_user.locale is free text in the schema, so it needs narrowing. */
export function isSupportedLanguage(value: string): value is SupportedLanguage {
  return (supportedLanguages as readonly string[]).includes(value)
}

void i18next.use(initReactI18next).init({
  resources: {
    en: { common: enCommon },
    sw: { common: swCommon },
  },
  lng: 'en',
  fallbackLng: 'en',
  defaultNS: 'common',
  ns: ['common'],
  interpolation: { escapeValue: false },
})

/**
 * `<html lang>` follows the active language — QA #18.
 *
 * `index.html` shipped `lang="sw"` and nothing ever changed it, so every ops
 * and admin screen — English by design — was announced to a screen reader with
 * Swahili pronunciation rules, which is close to unusable. Switching language
 * changed every rendered string and left the attribute alone.
 *
 * The static attribute in `index.html` matches `lng` below, so the first paint
 * is right before any of this runs.
 */
function applyDocumentLanguage(language: string) {
  if (typeof document === 'undefined') return
  document.documentElement.lang = language
}

i18next.on('languageChanged', applyDocumentLanguage)
applyDocumentLanguage(i18next.resolvedLanguage ?? 'en')

export default i18next
