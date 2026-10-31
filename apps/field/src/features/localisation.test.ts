import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, test } from 'vitest'

/**
 * QA #31 — the invariant the codebase kept breaking by hand.
 *
 * Crop and equipment names are translated in the DATABASE (`name_en` /
 * `name_sw`), so a query that picks one of them returns locale-dependent data.
 * Pick it inside a `queryFn` and the result is cached under that query's key —
 * and those keys are about the ROWS, not about the language. Whichever locale
 * won the race is then served for good.
 *
 * It showed as the Ilundo officer, whose locale is `sw`, seeing `Kahawa` in
 * the verify queue and `Maize` on a cycle detail in the same session: i18next
 * initialises to `en` and the stored locale arrives via an effect, so a query
 * that resolved first cached English.
 *
 * **The rule: a `queryFn` may not know the language.** Ask for both columns,
 * cache both, and choose at render — one cache entry, and a language switch
 * that takes effect immediately instead of after an invalidation.
 *
 * This is a source check rather than a behavioural one on purpose. The
 * behaviour is asserted for one hook in `useOfficerRecords.test.ts`; what
 * failed here was not a screen, it was a habit repeated across six queries, and
 * the next one will be written by someone who has not read this file.
 */

function sourceFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) return sourceFiles(path)
    if (!entry.name.endsWith('.ts') && !entry.name.endsWith('.tsx')) return []
    if (entry.name.includes('.test.')) return []
    return [path]
  })
}

/**
 * The body of every `queryFn`, roughly. Brace-matched from `queryFn:` to its
 * close, which is enough for this codebase's shapes:
 *   queryFn: async () => { … }
 *   queryFn: () => fetchThing(id, sw)
 */
function queryFnBodies(source: string): string[] {
  const bodies: string[] = []
  let index = source.indexOf('queryFn:')

  while (index !== -1) {
    const open = source.indexOf('{', index)
    const lineEnd = source.indexOf('\n', index)
    // A one-line arrow body: take the line.
    if (open === -1 || open > lineEnd) {
      bodies.push(source.slice(index, lineEnd === -1 ? source.length : lineEnd))
    } else {
      let depth = 0
      let cursor = open
      for (; cursor < source.length; cursor += 1) {
        if (source[cursor] === '{') depth += 1
        else if (source[cursor] === '}') {
          depth -= 1
          if (depth === 0) break
        }
      }
      bodies.push(source.slice(index, cursor + 1))
    }
    index = source.indexOf('queryFn:', index + 1)
  }

  return bodies
}

/**
 * Choosing a language, as opposed to asking for both columns.
 *
 * `crop ( name_en, name_sw )` inside a select string is correct and must stay
 * legal: it is how both names reach the cache in the first place.
 */
const CHOOSES_A_LANGUAGE = [
  /\bsw\s*\?/, // sw ? crop.name_sw : crop.name_en
  /\blanguage\s*===\s*'sw'/, // language === 'sw' ? …
  /\(\s*sw\s*\)/, // cropNames(sw)
  /,\s*sw\s*\)/, // fetchCycleDetail(cycleId, sw)
  /resolvedLanguage/,
]

describe('a queryFn never knows the language', () => {
  const files = sourceFiles('src/features')

  test('there are query functions to check', () => {
    const withQueries = files.filter((file) => readFileSync(file, 'utf8').includes('queryFn:'))
    expect(withQueries.length).toBeGreaterThan(5)
  })

  test.each(sourceFiles('src/features'))('%s', (file) => {
    const offenders = queryFnBodies(readFileSync(file, 'utf8')).filter((body) =>
      CHOOSES_A_LANGUAGE.some((pattern) => pattern.test(body)),
    )

    expect(
      offenders.map((body) => body.slice(0, 120)),
      'a queryFn that chooses a language caches one locale under a key that ignores it — ask for both columns and choose at render',
    ).toEqual([])
  })

  // The positive half: asking for both columns must stay legal, or the fix
  // itself would trip this guard.
  test('requesting both columns in a select is not choosing', () => {
    const bodies = queryFnBodies(`
      queryFn: async () => {
        const { data } = await supabase.from('crop_cycle').select('crop ( name_en, name_sw )')
        return data
      },
    `)
    expect(bodies).toHaveLength(1)
    expect(CHOOSES_A_LANGUAGE.some((p) => p.test(bodies[0]))).toBe(false)
  })

  test('and choosing is detected wherever it hides', () => {
    for (const body of [
      'queryFn: () => fetchCycleDetail(cycleId, sw),',
      'queryFn: async () => { return rows.map((r) => (sw ? r.name_sw : r.name_en)) }',
      'queryFn: async () => { const n = await cropNames(sw); return n }',
    ]) {
      expect(CHOOSES_A_LANGUAGE.some((p) => p.test(body)), body).toBe(true)
    }
  })
})
