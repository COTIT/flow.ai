import { describe, expect, test } from 'vitest'

import { code, offendingLines, read, sourceFiles } from './design'

/**
 * The redesign's token set, as delivered in `docs/design/tokens.css`, must
 * actually reach `src/styles/globals.css`. Tailwind v4 is CSS-first: there is
 * no config file, so this stylesheet is the whole design system and a token
 * that never lands here is a token no screen can use.
 */

const DELIVERED = 'docs/design/tokens.css'
const LIVE = 'src/styles/globals.css'

function customProperties(source: string): string[] {
  return [...source.matchAll(/^\s*(--[a-z0-9-]+)\s*:/gm)].map((match) => match[1])
}

describe('the delivered tokens reach globals.css', () => {
  const delivered = read(DELIVERED)
  const live = read(LIVE)

  // `@theme inline` aliases are re-declared in the live file against the same
  // names, so comparing the declared set is enough.
  const names = [...new Set(customProperties(delivered))]

  test('there are tokens to check', () => {
    expect(names.length).toBeGreaterThan(20)
  })

  test.each(names)('%s is declared', (name) => {
    expect(live).toContain(`${name}:`)
  })

  test('the ink scale replaces the opacity zoo', () => {
    for (const [token, value] of [
      ['--ink', '#0c1f5b'],
      ['--ink-2', '#46547d'],
      ['--ink-3', '#5d698b'],
      ['--primary-ink', '#14548a'],
      ['--green-ink', '#3f5410'],
      ['--flag-ink', '#9e1b1b'],
    ]) {
      expect(live, `${token} must be ${value}`).toMatch(
        new RegExp(`${token}\\s*:\\s*${value}`, 'i'),
      )
    }
  })

  // Diagonal stripes sit behind text and make it hard to read. Provisional is
  // said in words and weight, on a solid ground.
  test('there is no striped background token', () => {
    expect(live).not.toMatch(/--hatch\s*:/)
    expect(delivered).not.toMatch(/--hatch\s*:/)
  })

  test('the four radii are named', () => {
    for (const radius of [
      '--radius-control',
      '--radius-card',
      '--radius-frame',
      '--radius-pill',
    ]) {
      expect(live).toContain(`${radius}:`)
    }
  })

  test('the brand block is untouched', () => {
    for (const [token, value] of [
      ['--primary', '#1d70b7'],
      ['--accent', '#93c01f'],
      ['--deep', '#0c1f5b'],
      ['--surface', '#f5f3e5'],
    ]) {
      expect(live).toMatch(new RegExp(`${token}\\s*:\\s*${value}`, 'i'))
    }
  })
})

describe('the utilities the tests and screens depend on', () => {
  const live = read(LIVE)

  // `EnergyEstimatePanel.test.tsx` reads this class name off an element.
  test('the tabular utility keeps its name', () => {
    expect(live).toMatch(/\.tabular\s*\{[^}]*tabular-nums/)
  })

  test('the eight type steps are named', () => {
    for (const step of [
      'display',
      'figure',
      'title',
      'section',
      'body',
      'body-strong',
      'small',
      'note',
    ]) {
      expect(live, `type step "${step}" is missing`).toContain(`.type-${step}`)
    }
  })

  /**
   * 12px is the floor for everything a user reads for meaning. Two uppercase
   * micro-labels sit below it because the design file puts them there and the
   * implementation prompt says the design file wins where it and the prose
   * disagree — and this is where that exemption is allowed to live. Two, no
   * more: a third would mean the floor had stopped being a rule.
   */
  test('nothing in the type scale is below 12px, bar the two micro-labels', () => {
    const steps = [...live.matchAll(/\.(type-[a-z-]+)\s*\{([^}]*)\}/g)].map((match) => ({
      name: match[1],
      size: Number(/font-size:\s*([0-9.]+)px/.exec(match[2])?.[1] ?? Number.NaN),
    }))

    expect(steps.length).toBeGreaterThan(7)
    expect(steps.filter((step) => step.size < 12).map((step) => step.name)).toEqual([
      'type-column-label',
      'type-microlabel',
    ])
  })
})

describe('the responsive document frame', () => {
  const live = read(LIVE)

  test('prevents page-level overflow while table wrappers own horizontal scrolling', () => {
    expect(live).toMatch(/body\s*\{[^}]*overflow-x:\s*hidden/)
  })

  test('keeps text rendering crisp on the brand font', () => {
    expect(live).toMatch(/-webkit-font-smoothing:\s*antialiased/)
    expect(live).toMatch(/text-rendering:\s*optimizeLegibility/)
  })
})

describe('no dark palette is introduced', () => {
  const live = read(LIVE)

  // The `dark:` variant compiles so nothing breaks, but defining values for it
  // would be a product decision nobody has made.
  test('the dark variant defines no colours', () => {
    expect(live).not.toMatch(/\.dark\s*\{/)
    expect(live).not.toMatch(/prefers-color-scheme/)
  })
})

describe('no striped backgrounds', () => {
  // Stripes behind text cost legibility. A solid surface token does the job.
  test('no source file paints a repeating gradient or the old hatch token', () => {
    const offenders = sourceFiles('src').flatMap((path) =>
      offendingLines(code(path), /repeating-(linear|conic)-gradient|var\(--hatch\)/).map(
        (line) => `${path}:${line}`,
      ),
    )
    expect(offenders).toEqual([])
  })
})
