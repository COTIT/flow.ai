import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import i18next from 'i18next'

// The specs assert UI text through the app's own bundles, not English literals.
// The seeded farmers and officers have app_user.locale = 'sw', so once signed
// in the UI renders the Swahili bundle (falling back to English for any key it
// lacks); ops@ and admin@ are 'en'. `tr('sw', key)` therefore follows the
// bundle: an untranslated key still resolves to English, a translated one to
// its Swahili. Data that comes from the database (names, figures) is not here.
//
// The JSON is read with fs, like support/db.ts reads .env, so it does not
// depend on how Playwright's loader treats JSON imports.
const here = dirname(fileURLToPath(import.meta.url))
const bundle = (lng: 'en' | 'sw') =>
  JSON.parse(readFileSync(resolve(here, `../../src/i18n/${lng}/common.json`), 'utf8')) as Record<
    string,
    unknown
  >

const instance = i18next.createInstance()
void instance.init({
  resources: {
    en: { common: bundle('en') },
    sw: { common: bundle('sw') },
  },
  lng: 'en',
  fallbackLng: 'en',
  defaultNS: 'common',
  ns: ['common'],
  interpolation: { escapeValue: false },
  // Synchronous: resources are inline, so `tr` works at import time, inside
  // describe blocks and module-level arrays.
  initAsync: false,
})

export type Lng = 'en' | 'sw'

/** The bundle's string for `key` in `lng`, with i18next's own fallback to English. */
export function tr(lng: Lng, key: string, options?: Record<string, unknown>): string {
  return instance.getFixedT(lng)(key, options) as string
}

/** A string, escaped for use inside a RegExp. */
export function escapeRe(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * `tr`, escaped for a RegExp. `exact` anchors it (for a `getByRole` name);
 * `flags` are passed through, e.g. 'i' where the original matcher was /…/i;
 * `any` names interpolation values that vary at run time (a date, say), which
 * match anything instead of a fixed value.
 */
export function trRe(
  lng: Lng,
  key: string,
  options?: Record<string, unknown>,
  { exact = false, flags, any = [] }: { exact?: boolean; flags?: string; any?: string[] } = {},
): RegExp {
  const sentinel = (name: string) => `@@${name}@@`
  const values = { ...options, ...Object.fromEntries(any.map((n) => [n, sentinel(n)])) }
  let escaped = escapeRe(tr(lng, key, values))
  for (const name of any) escaped = escaped.replaceAll(sentinel(name), '.+')
  return new RegExp(exact ? `^${escaped}$` : escaped, flags)
}
