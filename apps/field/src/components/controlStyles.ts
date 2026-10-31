import type { CSSProperties } from 'react'

import type { Database } from '@/lib/db.types'

type VerificationStatus = Database['public']['Enums']['verification_status']

/**
 * The shared control surface.
 *
 * Two densities, and the difference is a product decision rather than a taste:
 * ops is read at a desk with a mouse, field surfaces are read outdoors with a
 * thumb. Both use `min-height` and never `height`, because a Kiswahili label
 * runs 10–30% longer than the English these were laid out in and has to be
 * allowed to wrap rather than be clipped.
 */

/** 40px. Ops. */
export const CONTROL: CSSProperties = {
  minHeight: 40,
  fontSize: 14,
  fontFamily: 'inherit',
  border: '1.5px solid var(--rule-2)',
  borderRadius: 'var(--radius-control)',
  background: 'var(--paper)',
  color: 'var(--ink)',
  padding: '8px 12px',
  boxSizing: 'border-box',
}

/** 48px. Officer and farmer. */
export const CONTROL_FIELD: CSSProperties = {
  ...CONTROL,
  minHeight: 48,
  fontSize: 16,
  padding: '12px 14px',
}

/** A primary action. Full width on a field surface, by the caller's class. */
export const BUTTON_PRIMARY: CSSProperties = {
  minHeight: 44,
  border: 0,
  borderRadius: 'var(--radius-control)',
  background: 'var(--primary)',
  color: '#fff',
  fontSize: 15,
  fontWeight: 600,
  fontFamily: 'inherit',
  padding: '10px 18px',
  textWrap: 'balance',
}

export const BUTTON_SECONDARY: CSSProperties = {
  minHeight: 44,
  border: '1.5px solid var(--rule-2)',
  borderRadius: 'var(--radius-control)',
  background: 'var(--paper)',
  color: 'var(--ink)',
  fontSize: 15,
  fontWeight: 500,
  fontFamily: 'inherit',
  padding: '10px 18px',
  textWrap: 'balance',
}

/**
 * The rail down a nested record, tinted by that record's own state.
 *
 * Six levels deep on a person detail, the rail is what lets the eye follow one
 * branch. Colour alone never carries the state — the mark beside it does that —
 * so this is a second, quieter signal rather than the only one.
 */
export function railColour(verification: VerificationStatus): string {
  switch (verification) {
    case 'verified':
      return 'var(--green-ink)'
    case 'pending':
      return 'var(--primary-ink)'
    case 'disputed':
      return 'var(--flag-ink)'
    default:
      return 'var(--rule-2)'
  }
}
