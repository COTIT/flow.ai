import type { CSSProperties } from 'react'

import type { Database } from '@/lib/db.types'

type VerificationStatus = Database['public']['Enums']['verification_status']
type ConfidenceLevel = Database['public']['Enums']['confidence_level']

/**
 * The mark vocabulary.
 *
 * One set of shapes, learned once and reused: a record's verification state, a
 * form group that is done, a figure that will be rounded, a field that failed.
 * Shape first and colour second, so every one of them survives greyscale, a
 * scratched screen and daylight.
 *
 * These are iconography, not text. The glyphs inside them are sized to their
 * disc — which is the one place in this build where type goes below the 12px
 * floor, and `typescale.test.ts` exempts this file by name for that reason and
 * no other.
 */

/**
 * Shape first, colour second. Each state is legible without colour, which is
 * the whole reason the marks exist rather than four coloured dots.
 */
export function VerificationMark({
  verification,
  size,
}: {
  verification: VerificationStatus
  size: number
}) {
  const base: CSSProperties = {
    width: size,
    height: size,
    borderRadius: 'var(--radius-pill)',
    boxSizing: 'border-box',
    flex: 'none',
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    lineHeight: 1,
  }
  const scale = size / 13

  if (verification === 'verified') {
    return (
      <span data-mark="verified" aria-hidden style={{ ...base, background: 'var(--green-ink)' }}>
        <span
          style={{
            display: 'block',
            width: 5 * scale,
            height: 2.5 * scale,
            borderLeft: `${2 * scale}px solid #fff`,
            borderBottom: `${2 * scale}px solid #fff`,
            transform: `rotate(-45deg) translate(${0.5 * scale}px, ${-1 * scale}px)`,
          }}
        />
      </span>
    )
  }

  if (verification === 'pending') {
    return (
      <span
        data-mark="pending"
        aria-hidden
        style={{
          ...base,
          border: `${2 * scale}px solid var(--primary-ink)`,
          background: 'linear-gradient(90deg, var(--primary-ink) 50%, transparent 50%)',
        }}
      />
    )
  }

  if (verification === 'disputed') {
    return (
      <span
        data-mark="disputed"
        aria-hidden
        style={{
          ...base,
          background: 'var(--flag-ink)',
          color: '#fff',
          fontSize: Math.max(9, Math.round(9 * scale)),
          fontWeight: 700,
        }}
      >
        !
      </span>
    )
  }

  return (
    <span
      data-mark="unverified"
      aria-hidden
      style={{ ...base, border: `${1.5 * scale}px dashed var(--ink-3)` }}
    />
  )
}

/**
 * Three bars, 5/8/11px tall. A meter reads as a quantity at a glance where a
 * fourth word read as more text to skip.
 */
export function ConfidenceMeter({
  confidence,
  label,
}: {
  confidence: ConfidenceLevel
  label: string
}) {
  const filled = CONFIDENCE_BARS[confidence]

  return (
    <>
      <span
        aria-hidden
        className="inline-flex items-end gap-[2px]"
        style={{ height: 11, flex: 'none' }}
      >
        {[5, 8, 11].map((height, index) => (
          <span
            key={height}
            data-bar={index < filled ? 'filled' : 'empty'}
            style={{
              width: 3,
              height,
              borderRadius: 1,
              background: index < filled ? 'var(--ink-2)' : 'var(--rule-2)',
            }}
          />
        ))}
      </span>
      <span className="sr-only">{label}</span>
    </>
  )
}


const CONFIDENCE_BARS: Record<ConfidenceLevel, number> = { low: 1, medium: 2, high: 3 }

/** Something failed. Filled disc, bang — the loudest mark in the set. */
export function BangMark({ size = 15 }: { size?: number }) {
  return (
    <span
      aria-hidden
      className="inline-flex items-center justify-center"
      style={{
        width: size,
        height: size,
        marginTop: 2,
        borderRadius: 'var(--radius-pill)',
        background: 'var(--flag-ink)',
        color: '#fff',
        fontSize: 10,
        fontWeight: 700,
        lineHeight: 1,
        flex: 'none',
      }}
    >
      !
    </span>
  )
}

/** Provisional: what is on screen is not yet what will be stored. */
export function PlannedMark({ size = 13 }: { size?: number }) {
  return (
    <span
      aria-hidden
      style={{
        width: size,
        height: size,
        borderRadius: 'var(--radius-pill)',
        background: 'var(--sand-2)',
        border: '1px solid var(--rule-2)',
        boxSizing: 'border-box',
        flex: 'none',
      }}
    />
  )
}
