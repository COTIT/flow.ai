import { describe, expect, test } from 'vitest'

import { read } from '@/styles/design'

/**
 * Without `viewport-fit=cover` every `env(safe-area-inset-*)` resolves to 0 on
 * iOS, and the tab bar is drawn under the home indicator. On Android it is also
 * the opt-in to edge-to-edge without Chrome's retracting "chin", which is what
 * keeps `safe-area-inset-bottom` stable. jsdom has no safe area, so the
 * mechanism is what is asserted.
 */
describe('the viewport opts into the full screen', () => {
  const html = read('index.html')
  const viewport = /<meta\s+name="viewport"\s+content="([^"]*)"/.exec(html)?.[1] ?? ''

  test('viewport-fit=cover is set, so env(safe-area-inset-*) reports the real insets', () => {
    expect(viewport).toMatch(/(^|,)\s*viewport-fit=cover\s*(,|$)/)
  })

  test('the device width and initial scale survive', () => {
    expect(viewport).toContain('width=device-width')
    expect(viewport).toContain('initial-scale=1')
  })

  // Full-screen with a translucent status bar would push the header under the
  // clock. The status bar keeps its own ground.
  test('the status bar is not made translucent', () => {
    expect(html).not.toContain('black-translucent')
  })
})

describe('the tab bar offset is one named token', () => {
  const css = read('src/styles/globals.css')

  test('--tab-bar-bottom keeps a floor and only grows where the hardware needs it', () => {
    const value = /--tab-bar-bottom\s*:\s*([^;]+);/.exec(css)?.[1] ?? ''

    expect(value).toMatch(/^max\(\s*12px\s*,/)
    expect(value).toContain('env(safe-area-inset-bottom')
  })
})
