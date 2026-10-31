import { describe, expect, test } from 'vitest'

import { code, offendingLines, sourceFiles } from './styles/design'

/**
 * Eight type steps, and 12px is the floor. Nothing smaller ships: the farmer
 * surface assumes the lowest literacy and the worst hardware in the programme,
 * and a 10px note on a cracked 5-inch screen in daylight is not information.
 *
 * Tailwind's own `text-xs` is exactly 12px, so what this catches is the
 * arbitrary value somebody reaches for when a row is one line too tall.
 */

const BELOW_FLOOR =
  /(^|[^\w-])text-\[\s*(?:([0-9]|1[01])(?:\.[0-9]+)?px|0?\.[0-6][0-9]*rem)\s*\]/

const RAW_FONT_SIZE = /font-size:\s*(?:([0-9]|1[01])(?:\.[0-9]+)?px|0?\.[0-6][0-9]*rem)/

/** The same thing written as a React style object. */
const STYLE_OBJECT_FONT_SIZE = /fontSize:\s*(?:'|")?(?:[0-9]|1[01])(?:\.[0-9]+)?(?:px)?(?:'|")?\s*[,}]/

/**
 * Two exemptions, both deliberate, both recorded here rather than left to
 * whoever reads the diff.
 *
 * `components/marks.tsx` — the mark vocabulary. The `disputed` mark is a filled
 * disc carrying a bang, and the bang is a glyph inside a 13px disc. It is
 * iconography sized to the mark, not a string anybody reads. Every mark in the
 * build lives in that one file, so this exemption cannot spread.
 *
 * Note what is NOT exempted: the sub-12px sizes the design file asks for on
 * column headers and tag pills live in exactly two utilities in globals.css,
 * `.type-column-label` and `.type-microlabel`, which `styles/tokens.test.ts`
 * pins to those two names. Screens reach them through the class, so this guard
 * stays strict everywhere and there is one place to change if the designer
 * confirms the 12px floor was meant to apply there too.
 */
const NOT_TEXT = new Set([
  'src/components/marks.tsx',
  // The type scale itself. `styles/tokens.test.ts` owns this file and pins the
  // two sub-12px steps to their exact names, which is a tighter rule than this
  // one — checking it twice here would only mean two places to edit.
  'src/styles/globals.css',
])

describe('nothing renders below 12px', () => {
  const files = sourceFiles('src')

  test('there are files to check', () => {
    expect(files.length).toBeGreaterThan(50)
  })

  test.each(files)('%s', (file) => {
    const source = code(file)
    const offenders = NOT_TEXT.has(file)
      ? []
      : [
          ...offendingLines(source, BELOW_FLOOR),
          ...offendingLines(source, RAW_FONT_SIZE),
          ...offendingLines(source, STYLE_OBJECT_FONT_SIZE),
        ]

    expect(offenders, '12px is the floor — see the type scale in globals.css').toEqual([])
  })
})

describe('the floor holds for the sizes the guard knows', () => {
  test('11px and 10px are caught', () => {
    expect(BELOW_FLOOR.test('className="text-[11px]"')).toBe(true)
    expect(BELOW_FLOOR.test('className="text-[10px]"')).toBe(true)
    expect(RAW_FONT_SIZE.test('font-size: 11px')).toBe(true)
    expect(STYLE_OBJECT_FONT_SIZE.test('{ fontSize: 9, fontWeight: 700 }')).toBe(true)
    expect(STYLE_OBJECT_FONT_SIZE.test("{ fontSize: '10px' }")).toBe(true)
  })

  test('12px and above are not', () => {
    expect(BELOW_FLOOR.test('className="text-[12px]"')).toBe(false)
    expect(BELOW_FLOOR.test('className="text-[15px]"')).toBe(false)
    expect(RAW_FONT_SIZE.test('font-size: 12px')).toBe(false)
    expect(RAW_FONT_SIZE.test('font-size: 30px')).toBe(false)
    expect(STYLE_OBJECT_FONT_SIZE.test('{ fontSize: 12 }')).toBe(false)
    expect(STYLE_OBJECT_FONT_SIZE.test("{ fontSize: '15px' }")).toBe(false)
  })
})
