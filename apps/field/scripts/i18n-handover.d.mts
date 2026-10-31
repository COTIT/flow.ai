/**
 * Types for `i18n-handover.mjs`.
 *
 * The script is plain JavaScript so `node scripts/i18n-handover.mjs` runs it
 * with no build step, while `src/i18n/handover.test.ts` still imports the pure
 * functions with real types.
 */

/** Nested translation namespaces flattened to i18next's dotted keys. */
export function flatten(source: unknown, prefix?: string): Record<string, string>

/**
 * `required` — farmer and officer surfaces, plus the chrome both render.
 * `optional` — ops and tower, which may ship English for the demo.
 */
export type Surface = 'required' | 'optional'

export function SURFACE_OF(key: string): Surface

/**
 * Problems in reviewed Swahili: unknown keys, changed interpolation
 * placeholders, and a plural variant missing its pair. Empty when clean.
 */
export function validateSwahili(en: unknown, sw: unknown): string[]

/** A batch of strings a named native reviewer has been through. */
export interface ReviewedBatch {
  reviewer: string
  /** YYYY-MM-DD. */
  date: string
  keys: string[]
}

/**
 * Problems in `reviewed.json`: a blank reviewer, a malformed date, a key with
 * no Swahili, or a key claimed by two batches. Empty when clean.
 */
export function validateReviewed(batches: ReviewedBatch[], sw: unknown): string[]

/** Strings under the namespaces whose Swahili matches `pattern`. */
export function wordingViolations(sw: unknown, namespaces: string[], pattern: RegExp): string[]

/** Every key any reviewer has signed off. */
export function reviewedKeysOf(batches: ReviewedBatch[]): Set<string>

/**
 * `missing` — no Swahili. `draft` — Swahili nobody has reviewed. `reviewed` —
 * listed in `reviewed.json`.
 */
export type ReviewStatus = 'missing' | 'draft' | 'reviewed'

export interface HandoverRow {
  key: string
  surface: Surface
  english: string
  /** The Swahili draft, empty until one exists. */
  swahili: string
  translated: boolean
  status: ReviewStatus
  /** i18next placeholders, which must survive translation intact. */
  placeholders: string[]
  /** A labelling rule from CLAUDE.md, where one governs this string. */
  note: string
  /** A drafter's or cross-check's doubt about this string, for the reviewer. */
  flag: string
}

export function buildHandover(
  en: unknown,
  sw: unknown,
  reviewedKeys?: Set<string>,
  flags?: Record<string, string>,
): HandoverRow[]

export function renderHandover(rows: HandoverRow[], options: { generated: string }): string

/** The same rows for a reviewer who would rather work in a spreadsheet. */
export function renderCsv(rows: HandoverRow[]): string

/** A short fingerprint of an English string; see `sw/en-source.json`. */
export function englishFingerprint(text: unknown): string
