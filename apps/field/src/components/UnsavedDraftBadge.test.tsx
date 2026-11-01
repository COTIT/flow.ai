import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { UnsavedDraftBadge } from '@/components/UnsavedDraftBadge'
import '@/i18n'

/**
 * An unsaved write must LOOK unsaved (business-rules §12).
 *
 * This badge appears on a phone that has just lost signal halfway through a
 * registration, in front of someone who is waiting. A treatment that read as
 * success there would be a lie, so the badge is red-edged and carries
 * a filled dot, on a solid ground.
 */
describe('UnsavedDraftBadge', () => {
  test('says the work is not submitted', () => {
    render(<UnsavedDraftBadge />)
    expect(screen.getByTestId('unsaved-draft-badge')).toHaveTextContent('Not yet submitted')
  })

  test('nothing about it looks like a success', () => {
    render(<UnsavedDraftBadge />)
    const style = screen.getByTestId('unsaved-draft-badge').getAttribute('style') ?? ''

    expect(style, 'solid ground, readable text').not.toMatch(/gradient|hatch/)
    expect(style, 'red-edged').toMatch(/var\(--flag-ink\)/)
    expect(style).not.toMatch(/var\(--accent\)|var\(--green/)
  })

  test('it carries a mark, not only a colour', () => {
    render(<UnsavedDraftBadge />)
    expect(screen.getByTestId('unsaved-draft-badge').querySelector('[data-mark="unsaved"]')).not.toBeNull()
  })
})
