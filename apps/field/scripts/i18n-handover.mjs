#!/usr/bin/env node
/**
 * Builds the Swahili review packet — `docs/i18n-handover.md` and
 * `docs/i18n-handover.csv`.
 *
 * CLAUDE.md: "Farmer and Officer surfaces ship complete Swahili. Ops and Tower
 * may ship English for the demo." On 29 Sep 2026 the product owner approved a
 * DRAFT Swahili so the demo is usable by people who do not read English. The
 * draft is not reviewed. This file is what a native reviewer receives: every
 * string with its draft, and a status column that says which strings a native
 * reader has actually been through (`src/i18n/sw/reviewed.json`).
 *
 * It exists because the string list is frozen: every screen is built, and the
 * keys will not move under the reviewer while they work.
 *
 *   node scripts/i18n-handover.mjs        # writes the .md and the .csv
 *
 * The pure functions are exported and unit-tested in src/i18n/handover.test.ts.
 */

import { createHash } from 'node:crypto'

/** Nested namespaces to dotted keys, which is how i18next addresses them. */
export function flatten(source, prefix = '') {
  const out = {}
  for (const [key, value] of Object.entries(source)) {
    const path = prefix ? `${prefix}.${key}` : key
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      Object.assign(out, flatten(value, path))
    } else {
      out[path] = value
    }
  }
  return out
}

/**
 * Namespaces that render on the Ops and Tower surfaces ONLY.
 *
 * Everything else — the farmer and officer screens, and the chrome both of
 * them render — is required, and so is anything unrecognised: a new
 * farmer-facing namespace must not fall off the reviewer's list by being
 * forgotten here.
 */
const OPTIONAL_NAMESPACES = new Set([
  'ops',
  'opsHome',
  'demand',
  'demandStatus',
  'opportunity',
  'buyers',
  'villages',
  'catalogue',
  'coverage',
  'tower',
])

/**
 * Namespaces or sub-trees that only ever render inside the ops shell, even
 * though their first path segment is shared or unfamiliar. `tour` is shared
 * chrome for farmer and officer, but `tour.ops` is the ops walkthrough.
 */
const OPTIONAL_PREFIXES = ['surveyAdmin', 'tour.ops']

export function SURFACE_OF(key) {
  if (OPTIONAL_NAMESPACES.has(key.split('.')[0])) return 'optional'
  return OPTIONAL_PREFIXES.some((prefix) => key === prefix || key.startsWith(`${prefix}.`))
    ? 'optional'
    : 'required'
}

/** Reviewed copy must preserve interpolation and plural variants. */
export function validateSwahili(en, sw) {
  const english = flatten(en)
  const translated = flatten(sw)
  const problems = []
  for (const [key, value] of Object.entries(translated)) {
    if (!(key in english)) { problems.push(`${key}: unknown key`); continue }
    const expected = placeholdersIn(english[key]).sort()
    const actual = placeholdersIn(value).sort()
    if (JSON.stringify(expected) !== JSON.stringify(actual)) problems.push(`${key}: interpolation placeholders differ`)
    if (key.endsWith('_one') && translated[`${key.slice(0,-4)}_other`] === undefined) problems.push(`${key}: plural _other missing`)
    if (key.endsWith('_other') && translated[`${key.slice(0,-6)}_one`] === undefined) problems.push(`${key}: plural _one missing`)
  }
  return problems
}

/**
 * CLAUDE.md's "Language and labelling" section, attached to the strings it
 * governs.
 *
 * These are product requirements, not copy preferences — "getting them wrong
 * misrepresents the programme" — and a reviewer who has not read CLAUDE.md
 * will render "estimate" as something confident and "indicative price" as a
 * quotation. Matched on the key path, so a note follows its string.
 */
const NOTES = [
  [
    /estimate|Estimate/,
    'Always labelled an ESTIMATE. Not a measurement, not a commitment.',
  ],
  [
    /indicative|price|Price/,
    'Prices are INDICATIVE, never quotations. Must not read as a firm offer.',
  ],
  [
    /^capacityBasis|capacity|Capacity|basis/,
    'Capacity is PLANNED or NAMEPLATE, never measured. Always shown with its basis.',
  ],
  [
    /notASale|opportunity/,
    'An opportunity is NOT a sale, a delivery or a payment. "Accepted" means both sides agreed to keep talking.',
  ],
  [
    /plantedArea|cycleArea|areaAcross/,
    'This is planted area ACROSS CYCLES, never "land area": intercropping means it can exceed the village’s hectares.',
  ],
  [
    /prospective|approved.*peak|peak/,
    'Prospective and approved demand are separate figures and are NEVER summed.',
  ],
  [/provenance|source|verification/, 'Provenance wording: where a record came from, and who verified it.'],
  [/confidence/, 'Confidence level recorded with a figure. Low / medium / high.'],
]

function noteFor(key) {
  for (const [pattern, note] of NOTES) {
    if (pattern.test(key)) return note
  }
  return ''
}

/** i18next interpolation, which has to survive translation intact. */
function placeholdersIn(value) {
  return String(value).match(/\{\{[^}]+\}\}/g) ?? []
}

/**
 * One row per English key.
 *
 * Required rows first: a reviewer's time is the scarce resource here, and the
 * farmer and officer surfaces are the ones that block the demo.
 *
 * `status` is `missing` (no Swahili), `draft` (Swahili nobody has reviewed) or
 * `reviewed` (listed in `reviewed.json`). Reviewed is a claim about a string
 * that exists, so a reviewed key with no Swahili is still `missing`.
 */
export function buildHandover(en, sw, reviewedKeys = new Set(), flags = {}) {
  const english = flatten(en)
  const swahili = flatten(sw)

  const rows = Object.entries(english).map(([key, value]) => {
    const translated = swahili[key] !== undefined
    return {
      key,
      surface: SURFACE_OF(key),
      english: String(value),
      swahili: translated ? String(swahili[key]) : '',
      translated,
      status: !translated ? 'missing' : reviewedKeys.has(key) ? 'reviewed' : 'draft',
      placeholders: placeholdersIn(value),
      note: noteFor(key),
      flag: flags[key] ?? '',
    }
  })

  return rows.sort((a, b) => {
    if (a.surface !== b.surface) return a.surface === 'required' ? -1 : 1
    return a.key.localeCompare(b.key)
  })
}

/** A pipe would otherwise close the column it sits in. */
function cell(text) {
  return String(text).replace(/\|/g, '\\|').replace(/\n/g, ' ')
}

export function renderHandover(rows, { generated }) {
  const required = rows.filter((r) => r.surface === 'required')
  const optional = rows.filter((r) => r.surface === 'optional')
  const count = (status) => rows.filter((r) => r.status === status).length

  const section = (title, subset) =>
    [
      `## ${title}`,
      '',
      '| Key | English | Swahili | Status | Notes |',
      '| --- | --- | --- | --- | --- |',
      ...subset.map((r) => {
        const notes = [
          r.note,
          r.placeholders.length ? `Keep ${r.placeholders.join(' ')}` : '',
          r.flag ? `Flag: ${r.flag}` : '',
        ]
          .filter(Boolean)
          .join(' ')
        return `| \`${r.key}\` | ${cell(r.english)} | ${cell(r.swahili)} | ${r.status} | ${cell(notes)} |`
      }),
      '',
    ].join('\n')

  return [
    '# Swahili handover — Ruaha 360',
    '',
    `Generated ${generated} from \`src/i18n/en/common.json\`. Do not edit by hand;`,
    'regenerate with `pnpm i18n:handover`.',
    '',
    '## What this is',
    '',
    'Every user-facing string in the application, with its Swahili **draft**,',
    'for a **native Kiswahili reviewer** to correct. The string list is frozen:',
    'every screen is built, so the keys will not move while the work is under',
    'way.',
    '',
    '**The Swahili here is an unreviewed draft.** It was written by Claude from',
    'a sourced glossary (`docs/i18n-glossary.md`) and back-translated blind as a',
    'cross-check. That catches some errors and not others, and a native reader',
    'has not seen it. It ships so the demo is usable in Swahili, with the',
    'status column recording honestly which strings a native reviewer has been',
    'through. Where a draft is wrong, correct it: a wrong Swahili string is',
    'worse than English, because English is visibly untranslated and a wrong',
    'string is not.',
    '',
    '## Priority',
    '',
    '- **Required** — the farmer and officer surfaces, and the chrome both of',
    `  them render. ${required.length} strings. CLAUDE.md specifies these ship`,
    '  complete Swahili.',
    `- **Optional** — Ops and Tower. ${optional.length} strings. These may ship`,
    '  English for the demo.',
    '',
    `${rows.length} strings in total: ${count('reviewed')} reviewed, ${count('draft')} draft, ${count('missing')} missing. ${rows.filter((r) => r.flag).length} flagged for a closer look (see the Flag note).`,
    '',
    '## How to read the table',
    '',
    '- **Status** is `draft` until a native reviewer has been through the',
    '  string, `reviewed` once they have, and `missing` if there is no Swahili',
    '  yet.',
    '- `{{value}}` and its siblings are placeholders. They must appear in the',
    '  translation exactly as written, or the string will render broken.',
    '- The Notes column carries product requirements from CLAUDE.md, not style',
    '  advice. "Estimate", "indicative" and "planned capacity" are claims about',
    '  what the programme does and does not promise; a translation that',
    '  strengthens them misrepresents it.',
    '- A survey incentive is a fixed cash amount paid at the office. It is never',
    '  earnings, a wallet, a balance or a payment.',
    '- Terms marked `unverified` in `docs/i18n-glossary.md` had no Tanzanian',
    '  source. Check those first.',
    '- Flag anything whose meaning is unclear rather than guessing. A gap is a',
    '  question; a wrong string is a defect nobody sees.',
    '',
    section('Required — farmer and officer surfaces', required),
    section('Optional — ops and tower surfaces', optional),
  ].join('\n')
}

/** A field holding a comma, a quote or a newline would shift every column. */
function csvField(text) {
  const value = String(text)
  return /[",\n]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value
}

/** The same rows for a reviewer who would rather work in a spreadsheet. */
export function renderCsv(rows) {
  const header = 'key,surface,english,swahili,status,notes'
  const lines = rows.map((r) =>
    [r.key, r.surface, r.english, r.swahili, r.status, [r.note, r.flag ? `Flag: ${r.flag}` : ''].filter(Boolean).join(' ')]
      .map(csvField)
      .join(','),
  )
  return [header, ...lines].join('\n')
}

/**
 * `reviewed.json` is an array of `{reviewer, date, keys[]}`. A key on it is a
 * claim that a native reader supplied or approved the string, so a batch has to
 * name who and when, and every key has to be a string that exists.
 */
export function validateReviewed(batches, sw) {
  const swahili = flatten(sw)
  const problems = []
  const claimedIn = new Map()
  batches.forEach((batch, index) => {
    const label = `batch ${index + 1}`
    if (typeof batch.reviewer !== 'string' || batch.reviewer.trim() === '') {
      problems.push(`${label}: reviewer is blank`)
    }
    if (!/^\d{4}-\d{2}-\d{2}$/.test(String(batch.date))) {
      problems.push(`${label}: date must be YYYY-MM-DD`)
    }
    for (const key of batch.keys) {
      if (!(key in swahili)) problems.push(`${label}: ${key} has no Swahili`)
      else if (claimedIn.has(key)) {
        problems.push(`${label}: ${key} is already reviewed in ${claimedIn.get(key)}`)
      } else claimedIn.set(key, label)
    }
  })
  return problems
}

/**
 * Strings under the given top-level namespaces whose Swahili matches `pattern`.
 * Used to keep the incentive wording out of earnings/wallet/balance/payment.
 */
export function wordingViolations(sw, namespaces, pattern) {
  return Object.entries(flatten(sw))
    .filter(([key]) => namespaces.includes(key.split('.')[0]))
    .filter(([, value]) => pattern.test(String(value)))
    .map(([key, value]) => `${key}: "${value}"`)
}

/**
 * A short fingerprint of an English string, kept beside its Swahili in
 * `en-source.json` so a later change to the English is noticed.
 */
export function englishFingerprint(text) {
  return createHash('sha1').update(String(text)).digest('hex').slice(0, 10)
}

/** Every key any reviewer has signed off, from `reviewed.json`. */
export function reviewedKeysOf(batches) {
  return new Set(batches.flatMap((batch) => batch.keys))
}

// ── CLI ──────────────────────────────────────────────────────────────
// Only when run directly, so importing the pure functions writes nothing.
if (process.argv[1] && process.argv[1].endsWith('i18n-handover.mjs')) {
  const { readFileSync, writeFileSync } = await import('node:fs')
  const { resolve, dirname } = await import('node:path')
  const { fileURLToPath } = await import('node:url')

  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
  const read = (path) => JSON.parse(readFileSync(resolve(root, path), 'utf8'))

  const en = read('src/i18n/en/common.json')
  const sw = read('src/i18n/sw/common.json')
  const batches = read('src/i18n/sw/reviewed.json')
  const reviewed = reviewedKeysOf(batches)
  const flags = read('src/i18n/sw/flags.json')
  const problems = [...validateSwahili(en, sw), ...validateReviewed(batches, sw)]
  if (problems.length) throw new Error(problems.join('\n'))
  const rows = buildHandover(en, sw, reviewed, read('src/i18n/sw/flags.json'))
  const out = resolve(root, 'docs/i18n-handover.md')
  writeFileSync(
    out,
    renderHandover(rows, { generated: new Date().toISOString().slice(0, 10) }) + '\n',
  )
  writeFileSync(resolve(root, 'docs/i18n-handover.csv'), renderCsv(rows) + '\n')

  const required = rows.filter((r) => r.surface === 'required').length
  console.log(`wrote ${out} — ${rows.length} strings, ${required} required`)
}
