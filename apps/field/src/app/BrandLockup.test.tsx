import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { BrandLockup } from '@/app/BrandLockup'
import '@/i18n'

/**
 * Ruaha 360 is a programme surface, not a second brand. The supplied lockup
 * stays whole and "360" follows it after a hairline divider, set in the
 * interface's own type rather than dressed up as a logo of its own.
 */
describe('BrandLockup', () => {
  test('the lockup names Ruaha Energy, and 360 follows it', () => {
    render(<BrandLockup height={26} />)

    expect(screen.getByAltText('Ruaha Energy')).toBeInTheDocument()
    expect(screen.getByText('360')).toBeInTheDocument()
  })

  test('it is drawn at the height it was asked for', () => {
    render(<BrandLockup height={23} />)
    expect(screen.getByAltText('Ruaha Energy').getAttribute('style')).toMatch(/height:\s*23px/)
  })

  // Below 20px the wordmark stops being legible, so the component refuses
  // rather than rendering something nobody can read.
  test('it will not draw itself below 20px', () => {
    expect(() => render(<BrandLockup height={16} />)).toThrow(/20px/)
  })
})
