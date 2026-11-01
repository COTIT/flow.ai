import { describe, expect, test } from 'vitest'

import { code, offendingLines, sourceFiles } from './styles/design'

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
