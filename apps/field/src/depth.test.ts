import { describe, expect, test } from 'vitest'

import { code, offendingLines, sourceFiles } from './styles/design'

/**
 * Depth is borders and white-on-sand contrast. No shadow, no filter, no blur —
 * each of them costs a compositing layer on hardware that cannot spare one, and
 * the design does not need them: the sand ground does the separating.
 */

const DEPTH = [
  /\bbox-shadow\s*:/,
  /\bbackdrop-filter\s*:/,
  /(^|[^\w-])filter\s*:\s*(blur|brightness|contrast|drop-shadow|grayscale|hue-rotate|invert|opacity|saturate|sepia|url)\(/,
  /(^|[^\w-])(shadow|blur|backdrop-blur|drop-shadow)-[a-z0-9[]/,
  /(^|[^\w-])shadow(\s|"|'|`|$)/,
]

describe('depth is borders, not shadows', () => {
  const files = sourceFiles('src')

  test.each(files)('%s', (file) => {
    const source = code(file)
    const offenders = DEPTH.flatMap((pattern) => offendingLines(source, pattern))

    expect(
      offenders,
      'no shadow, no filter, no blur — depth is a border and a white card on the sand ground',
    ).toEqual([])
  })
})

describe('no images beyond the brand mark', () => {
  const files = sourceFiles('src', ['.ts', '.tsx'])

  test.each(files)('%s', (file) => {
    const source = code(file)
    const images = offendingLines(source, /\.(png|jpe?g|gif|webp|avif)\b/i)

    expect(images, 'icons are inline lucide components; marks and meters are CSS').toEqual([])
  })
})
