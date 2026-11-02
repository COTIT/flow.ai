import { describe, expect, test } from 'vitest'

import { code, offendingLines, read, sourceFiles } from './styles/design'

/**
 * `text-deep/60` on the sand ground measures 4.17:1 and was used for every
 * secondary string in the product. It is the redesign's headline accessibility
 * fix, and the fix is not "raise that one opacity" — it is that colour stops
 * being expressed as an opacity at all. Secondary text is `--ink-2` (6.6:1),
 * notes are `--ink-3` (4.8:1), and a tint is a named token rather than a
 * number somebody chose in the moment.
 *
 * This ran as a ratchet through the redesign, with a shrinking list of files
 * still to convert. The list reached zero, so the rule now simply holds for
 * every file — which is the point a ratchet is built to arrive at.
 */

const OPACITY = /(^|[^\w-])(text|bg|border|fill|stroke|ring|divide|outline)-(deep|white|primary|accent|destructive|black|foreground|muted)\/[0-9]+/

describe('colour is a named token, never an opacity', () => {
  test.each(sourceFiles('src', ['.ts', '.tsx']))('%s', (file) => {
    expect(
      offendingLines(code(file), OPACITY),
      'use --ink-2 for secondary text, --ink-3 for notes, and the named tints — not an opacity',
    ).toEqual([])
  })
})

/**
 * The shared Notice (packages/ui) draws its title and body in `--foreground`
 * on its tone at a low alpha (`bg-info/10`, `bg-warning/15`, ...), edged in
 * the tone itself. Read both from source so a new tone or a heavier tint is
 * checked too: text at least 4.5:1, the edge at least 3:1, on sand and paper.
 */
describe('the Notice tones are legible on the RUAHA grounds', () => {
  const root = /:root\s*\{([^}]*)\}/.exec(read('src/styles/globals.css'))?.[1] ?? ''
  const tokens = Object.fromEntries(
    [...root.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]),
  )
  const resolve = (value: string): string => {
    const ref = /^var\((--[a-z0-9-]+)\)$/.exec(value)
    return ref ? resolve(tokens[ref[1]] ?? '') : value
  }
  const rgb = (value: string) => {
    const hex = resolve(value)
    expect(hex, `${value} must resolve to a #rrggbb colour`).toMatch(/^#[0-9a-f]{6}$/i)
    return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16))
  }
  const luminance = (c: number[]) =>
    c
      .map((v) => v / 255)
      .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
      .reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0)
  const ratio = (a: number[], b: number[]) => {
    const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
    return (hi + 0.05) / (lo + 0.05)
  }
  const over = (tone: number[], ground: number[], alpha: number) =>
    tone.map((v, i) => v * alpha + ground[i] * (1 - alpha))

  const notice = read('../../packages/ui/src/components/notice.tsx')
  const tones = [...notice.matchAll(/bg-([a-z]+)\/(\d+)/g)].map((m) => ({
    token: `--${m[1]}`,
    alpha: Number(m[2]) / 100,
  }))

  test('the four tones are read from the Notice', () => {
    expect(tones.map((tone) => tone.token).sort()).toEqual(['--destructive', '--info', '--success', '--warning'])
  })

  test.each(tones)('$token', ({ token, alpha }) => {
    for (const ground of ['--sand', '--paper']) {
      const fill = over(rgb(`var(${token})`), rgb(`var(${ground})`), alpha)
      expect(ratio(rgb('var(--foreground)'), fill), `text on ${token} over ${ground}`).toBeGreaterThanOrEqual(4.5)
      expect(ratio(rgb(`var(${token})`), fill), `${token} edge over ${ground}`).toBeGreaterThanOrEqual(3)
    }
  })
})
