import { describe, expect, test } from 'vitest'

import { code, offendingLines, read, sourceFiles } from './styles/design'

/**
 * Nothing animates and nothing transitions. This is a field constraint, not a
 * preference: users are on low- to mid-spec Android phones over 3G and the PWA
 * precaches the shell. A state change is a repaint, not a performance.
 *
 * The only legal occurrences are the two global kill rules in globals.css.
 */

const MOTION = [
  /@keyframes/,
  /\banimation\s*:(?!\s*none\b)/,
  /\btransition\s*:(?!\s*none\b)/,
  /(^|[^\w-])(transition|animate|duration|ease-in|ease-out|ease-linear|delay)-[a-z0-9[]/,
  /\bmotion-(safe|reduce)\b/,
]

describe('nothing in src animates', () => {
  const files = sourceFiles('src')

  test('there are files to check', () => {
    expect(files.length).toBeGreaterThan(50)
  })

  test.each(files)('%s', (file) => {
    const source = code(file)
    const offenders = MOTION.flatMap((pattern) => offendingLines(source, pattern))

    expect(
      offenders,
      'no animation and no transition anywhere — a phone on 3G has better things to repaint',
    ).toEqual([])
  })
})

describe('the kill switch is in place', () => {
  const live = read('src/styles/globals.css')

  test('animation and transition are disabled globally', () => {
    expect(live).toMatch(/animation:\s*none\s*!important/)
    expect(live).toMatch(/transition:\s*none\s*!important/)
  })

  test('no animation library is imported', () => {
    expect(live).not.toContain('tw-animate-css')
  })
})
