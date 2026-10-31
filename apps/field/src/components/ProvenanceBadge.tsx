import type { CSSProperties } from 'react'
import { useTranslation } from 'react-i18next'

import { ConfidenceMeter, VerificationMark } from '@/components/marks'

import { formatTimestamp } from '@/lib/format'
import type { Database } from '@/lib/db.types'

type SourceType = Database['public']['Enums']['source_type']
type VerificationStatus = Database['public']['Enums']['verification_status']
type ConfidenceLevel = Database['public']['Enums']['confidence_level']

export interface ProvenanceBadgeProps {
  source: SourceType
  verification: VerificationStatus
  confidence?: ConfidenceLevel | null
  capturedAt: string | null
  capturedBy?: string | null
  /** One line instead of a lozenge, for a badge that repeats down a list. */
  compact?: boolean
  /** What kind of record this is — "Person", "Plot". Compact variant only. */
  recordLabel?: string
}

/**
 * One chip carrying where a record's data came from — spec §9.1.
 *
 * Two segments, not three chips. The source is quiet: a colour cap and a word,
 * tinted by category. The verification is loud, and carries a **mark whose
 * shape differs** — a filled disc with a check, a half-filled ring, an empty
 * dashed ring, a filled disc with a bang — so a column of records can be read
 * for state at a glance, in greyscale, and `verified` can never be mistaken for
 * `unverified`. Confidence became a three-bar meter because as a third word it
 * was competing with verification for the same attention; the word itself stays
 * for screen readers.
 *
 * This component is how the reported / verified / measured / estimated
 * discipline actually reaches users, so the distinctions are kept sharp: the
 * five source values each get their own treatment via `data-source`,
 * verification is exposed separately via `data-verification`, and an estimate
 * is always labelled an estimate rather than shading into "measured".
 *
 * Appears on every record on every surface.
 */
export function ProvenanceBadge({
  source,
  verification,
  confidence,
  capturedAt,
  capturedBy,
  compact = false,
  recordLabel,
}: ProvenanceBadgeProps) {
  const { t } = useTranslation()

  const sourceLabel = t(`source.${source}`)
  const verificationLabel = t(`verification.${verification}`)
  const captured = formatTimestamp(capturedAt)

  // Long form for the tooltip. capturedBy is omitted when unknown rather than
  // rendered as "by undefined".
  const title = [
    sourceLabel,
    verificationLabel,
    confidence ? `${t('provenance.confidenceLabel')}: ${t(`confidence.${confidence}`)}` : null,
    capturedBy
      ? `${t('provenance.capturedAt')} ${captured} ${t('provenance.capturedBy')} ${capturedBy}`
      : `${t('provenance.capturedAt')} ${captured}`,
  ]
    .filter(Boolean)
    .join(' · ')

  if (compact) {
    // In a list the mark leads and the words collapse to one line. Same
    // component, same attributes — only the density changes.
    return (
      <span
        data-testid="provenance-badge"
        data-source={source}
        data-verification={verification}
        title={title}
        className="type-note inline-flex min-w-0 items-center gap-2"
        style={{ color: 'var(--ink-3)' }}
      >
        <VerificationMark verification={verification} size={18} />
        <span data-testid="provenance-verification" className="sr-only">
          {verificationLabel}
        </span>
        <span className="min-w-0">
          {[recordLabel, sourceLabel, `${t('provenance.capturedAt')} ${captured}`]
            .filter(Boolean)
            .join(' · ')}
        </span>
        {confidence && <ConfidenceMeter confidence={confidence} label={t(`confidence.${confidence}`)} />}
      </span>
    )
  }

  const treatment = SOURCE_TREATMENT[source]

  return (
    <span
      data-testid="provenance-badge"
      data-source={source}
      data-verification={verification}
      title={title}
      className="type-note inline-flex max-w-full items-stretch overflow-hidden"
      style={{
        border: '1px solid var(--rule-2)',
        borderRadius: 'var(--radius-pill)',
        background: 'var(--paper)',
      }}
    >
      <span
        className="inline-flex items-center gap-[7px] px-2.5 py-[5px] font-medium"
        style={treatment.segment}
      >
        <span className="flex-none" style={{ width: 4, height: 13, borderRadius: 2, ...treatment.cap }} />
        {sourceLabel}
      </span>

      <span
        data-testid="provenance-verification"
        className="inline-flex items-center gap-[5px] px-2.5 py-[5px] font-semibold"
        style={{ borderLeft: '1px solid var(--rule-2)', ...VERIFICATION_SEGMENT[verification] }}
      >
        <VerificationMark verification={verification} size={13} />
        {verificationLabel}
      </span>

      {confidence && (
        <span
          data-testid="provenance-confidence"
          className="inline-flex items-center gap-[5px] px-2.5 py-[5px]"
          style={{ borderLeft: '1px solid var(--rule-2)', color: 'var(--ink-2)' }}
        >
          <ConfidenceMeter confidence={confidence} label={t(`confidence.${confidence}`)} />
        </span>
      )}
    </span>
  )
}

/**
 * Five sources, five treatments: blue solid, plain, sunken sand, green,
 * italic.
 */
const SOURCE_TREATMENT: Record<SourceType, { segment: CSSProperties; cap: CSSProperties }> = {
  field_verified: {
    segment: { background: 'var(--primary-tint)', color: 'var(--primary-ink)' },
    cap: { background: 'var(--primary)' },
  },
  farmer_reported: {
    segment: { color: 'var(--ink-2)' },
    cap: { background: 'var(--ink-2)' },
  },
  transaction_derived: {
    segment: { background: 'var(--sand-2)', color: 'var(--ink-2)' },
    cap: { background: 'var(--ink)' },
  },
  sensor_derived: {
    segment: { background: 'var(--green-tint)', color: 'var(--green-ink)' },
    cap: { background: 'var(--accent)' },
  },
  model_estimated: {
    segment: {
      background: 'var(--paper)',
      color: 'var(--ink-2)',
      fontStyle: 'italic',
    },
    cap: { background: 'var(--ink-3)' },
  },
}

const VERIFICATION_SEGMENT: Record<VerificationStatus, CSSProperties> = {
  unverified: { color: 'var(--ink-3)' },
  pending: { color: 'var(--primary-ink)' },
  verified: { color: 'var(--green-ink)' },
  disputed: { color: 'var(--flag-ink)', background: 'var(--flag-tint)' },
}
