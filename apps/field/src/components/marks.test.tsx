import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { ProvenanceBadge } from '@/components/ProvenanceBadge'
import '@/i18n'

const AT = '2026-09-09T21:30:00Z'

/**
 * Verification state may never be carried by colour alone.
 *
 * Every state therefore has its own **shape**: a filled disc with a check, a
 * half-filled ring, an empty dashed ring, a filled disc with a bang. A column
 * of records can then be read for state at a glance, in greyscale, by someone
 * who cannot tell the green from the blue — and `verified` can never be
 * mistaken for `unverified`, which is the failure that matters.
 */

const STATES = ['unverified', 'pending', 'verified', 'disputed'] as const

function mark(verification: (typeof STATES)[number]) {
  const { unmount } = render(
    <ProvenanceBadge source="farmer_reported" verification={verification} capturedAt={AT} />,
  )
  const element = screen.getByTestId('provenance-badge').querySelector('[data-mark]')
  return { element: element as HTMLElement, unmount }
}

describe('the four verification marks differ in shape, not only in colour', () => {
  test.each(STATES)('%s renders a mark of its own', (verification) => {
    const { element, unmount } = mark(verification)
    expect(element, 'every verification state renders a mark').not.toBeNull()
    expect(element.getAttribute('data-mark')).toBe(verification)
    unmount()
  })

  test('all four marks are structurally distinct', () => {
    const shapes = new Set<string>()
    for (const verification of STATES) {
      const { element, unmount } = mark(verification)
      shapes.add(`${element.getAttribute('style') ?? ''}|${element.innerHTML}`)
      unmount()
    }
    expect(shapes.size, 'two states share a mark — the shape system has collapsed').toBe(4)
  })

  test('verified is a filled disc carrying a check', () => {
    const { element, unmount } = mark('verified')
    expect(element.getAttribute('style')).toMatch(/border-radius:\s*(999px|var\(--radius-pill\))/)
    expect(element.children.length, 'the check is a child element').toBeGreaterThan(0)
    unmount()
  })

  test('pending is a half-filled ring', () => {
    const { element, unmount } = mark('pending')
    expect(element.getAttribute('style')).toMatch(/linear-gradient/)
    unmount()
  })

  test('unverified is an empty dashed ring', () => {
    const { element, unmount } = mark('unverified')
    expect(element.getAttribute('style')).toMatch(/dashed/)
    expect(element.textContent).toBe('')
    unmount()
  })

  test('disputed is a filled disc carrying a bang', () => {
    const { element, unmount } = mark('disputed')
    expect(element.textContent).toBe('!')
    unmount()
  })

  // The one confusion the design exists to prevent.
  test('verified and unverified share nothing', () => {
    const verified = mark('verified')
    const verifiedShape = verified.element.getAttribute('style') ?? ''
    verified.unmount()

    const unverified = mark('unverified')
    const unverifiedShape = unverified.element.getAttribute('style') ?? ''
    unverified.unmount()

    expect(verifiedShape).not.toBe(unverifiedShape)
    expect(unverifiedShape).toMatch(/dashed/)
    expect(verifiedShape).not.toMatch(/dashed/)
  })
})

describe('confidence is a meter, not a third word competing for attention', () => {
  const LEVELS = [
    ['low', 1],
    ['medium', 2],
    ['high', 3],
  ] as const

  test.each(LEVELS)('%s fills %d of three bars', (confidence, filled) => {
    render(
      <ProvenanceBadge
        source="farmer_reported"
        verification="unverified"
        confidence={confidence}
        capturedAt={AT}
      />,
    )
    const meter = screen.getByTestId('provenance-confidence')
    expect(meter.querySelectorAll('[data-bar]')).toHaveLength(3)
    expect(meter.querySelectorAll('[data-bar="filled"]')).toHaveLength(filled)
  })

  // The meter is for the eye. The word still has to reach a screen reader.
  test('the text label survives for screen readers', () => {
    render(
      <ProvenanceBadge
        source="farmer_reported"
        verification="unverified"
        confidence="low"
        capturedAt={AT}
      />,
    )
    expect(screen.getByTestId('provenance-confidence')).toHaveTextContent('Low')
  })
})

describe('the compact variant, for a list that repeats the badge down the page', () => {
  test('the mark leads and the words collapse to one line', () => {
    render(
      <ProvenanceBadge
        compact
        recordLabel="Person"
        source="farmer_reported"
        verification="unverified"
        capturedAt={AT}
      />,
    )
    const badge = screen.getByTestId('provenance-badge')
    expect(badge.querySelector('[data-mark]')).not.toBeNull()
    expect(badge).toHaveTextContent('Person')
    expect(badge).toHaveTextContent('Farmer reported')
  })

  test('it is still one badge, with the same attributes tests select on', () => {
    render(
      <ProvenanceBadge
        compact
        source="field_verified"
        verification="verified"
        capturedAt={AT}
      />,
    )
    const badge = screen.getByTestId('provenance-badge')
    expect(badge).toHaveAttribute('data-source', 'field_verified')
    expect(badge).toHaveAttribute('data-verification', 'verified')
    expect(screen.getByTestId('provenance-verification')).toHaveTextContent('Verified')
  })
})
