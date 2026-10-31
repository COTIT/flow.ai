import type { CSSProperties } from 'react'
import { useTranslation } from 'react-i18next'

import type { Database } from '@/lib/db.types'

type RequestStatus = Database['public']['Enums']['pue_status']
type DemandStatus = Database['public']['Enums']['demand_status']
type OpportunityStatus = Database['public']['Enums']['opportunity_status']
type SurveyStatus = Database['public']['Enums']['survey_status']
/** `expired` is derived from `expires_at`; the database never stores it. */
export type VoucherDisplayStatus = Database['public']['Enums']['voucher_status'] | 'expired'

export type StatusPillProps =
  | { kind: 'request'; status: RequestStatus }
  | { kind: 'demand'; status: DemandStatus }
  | { kind: 'opportunity'; status: OpportunityStatus }
  | { kind: 'survey'; status: SurveyStatus }
  | { kind: 'voucher'; status: VoucherDisplayStatus }

const NAMESPACE = {
  request: 'requestStatus',
  demand: 'demandStatus',
  opportunity: 'opportunityStatus',
  survey: 'surveyStatus',
  voucher: 'voucherStatus',
} as const

/**
 * One pill for request, demand and opportunity statuses — spec §9.3.
 *
 * The three kinds keep separate translation namespaces on purpose: an
 * opportunity's 'accepted' means both sides agreed to talk further and nothing
 * has moved, which is a different claim from a request being approved. Sharing
 * a label between them would blur exactly the distinction the schema exists to
 * keep — and until the redesign they also shared a treatment, which blurred it
 * again in the one place users actually look.
 *
 * So: a request's **approved** is the only solid fill in the product, because
 * it is the only state that is a decision with capacity consequences. Every
 * opportunity pill is outlined on white, and **accepted** is distinguished by a
 * heavier edge rather than by a fill. Statuses use quiet solid surfaces only, so every
 * label remains easy to read.
 */
export function StatusPill(props: StatusPillProps) {
  const { t } = useTranslation()
  const { kind, status } = props

  return (
    <span
      data-testid="status-pill"
      data-status={status}
      className="type-note inline-flex items-center px-[11px] py-1"
      style={{ borderRadius: 'var(--radius-pill)', ...styleFor(props) }}
    >
      {t(`${NAMESPACE[kind]}.${status}`)}
    </span>
  )
}

/** Outlined on bare paper: the quietest pill in the system. */
const OUTLINE: CSSProperties = {
  border: '1px solid var(--rule-2)',
  background: 'var(--paper)',
  color: 'var(--ink-2)',
  fontWeight: 500,
}

/** Provisional, and going nowhere. */
const PROVISIONAL: CSSProperties = {
  border: '1px solid var(--rule-2)',
  background: 'var(--sand-2)',
  color: 'var(--ink-2)',
  fontWeight: 500,
}

const REQUEST_STYLE: Record<RequestStatus, CSSProperties> = {
  draft: { ...OUTLINE, color: 'var(--ink-3)' },
  submitted: {
    border: '1px solid rgba(29, 112, 183, .35)',
    background: 'var(--primary-tint)',
    color: 'var(--primary-ink)',
    fontWeight: 500,
  },
  under_review: {
    border: '1px solid var(--primary)',
    background: 'var(--primary-tint)',
    color: 'var(--primary-ink)',
    fontWeight: 600,
  },
  // The one fill.
  approved: {
    border: '1px solid var(--green-ink)',
    background: 'var(--accent)',
    color: '#22300a',
    fontWeight: 600,
  },
  rejected: {
    border: '1px solid rgba(158, 27, 27, .4)',
    background: 'var(--flag-tint)',
    color: 'var(--flag-ink)',
    fontWeight: 600,
  },
  withdrawn: { ...PROVISIONAL, background: 'var(--sand-2)', color: 'var(--ink-2)' },
}

const DEMAND_STYLE: Record<DemandStatus, CSSProperties> = {
  open: {
    border: '1px solid rgba(29, 112, 183, .35)',
    background: 'var(--primary-tint)',
    color: 'var(--primary-ink)',
    fontWeight: 500,
  },
  // A tint, not a fill: supply was found, nothing was sold.
  matched: {
    border: '1px solid var(--green-ink)',
    background: 'var(--green-tint)',
    color: 'var(--green-ink)',
    fontWeight: 600,
  },
  closed: { ...OUTLINE, background: 'var(--sand-2)' },
  cancelled: { ...PROVISIONAL, background: 'var(--sand-2)', color: 'var(--ink-2)' },
}

const OPPORTUNITY_STYLE: Record<OpportunityStatus, CSSProperties> = {
  proposed: OUTLINE,
  shared: {
    border: '1px solid rgba(29, 112, 183, .35)',
    background: 'var(--paper)',
    color: 'var(--primary-ink)',
    fontWeight: 500,
  },
  // A heavier edge, and nothing else. Not an approval.
  accepted: {
    border: '1.5px solid var(--green-ink)',
    background: 'var(--paper)',
    color: 'var(--green-ink)',
    fontWeight: 600,
  },
  declined: {
    border: '1px solid rgba(158, 27, 27, .4)',
    background: 'var(--paper)',
    color: 'var(--flag-ink)',
    fontWeight: 500,
  },
  lapsed: { ...PROVISIONAL, background: 'var(--sand-2)', color: 'var(--ink-2)' },
}

const SURVEY_STYLE: Record<SurveyStatus, CSSProperties> = {
  draft: { ...OUTLINE, color: 'var(--ink-3)' },
  live: {
    border: '1px solid var(--primary)',
    background: 'var(--primary-tint)',
    color: 'var(--primary-ink)',
    fontWeight: 600,
  },
  closed: { ...OUTLINE, background: 'var(--sand-2)' },
}

// A collected incentive is money handed over, not a decision with capacity
// consequences, so it takes a tint rather than the approval fill.
const VOUCHER_STYLE: Record<VoucherDisplayStatus, CSSProperties> = {
  issued: {
    border: '1px solid rgba(29, 112, 183, .35)',
    background: 'var(--primary-tint)',
    color: 'var(--primary-ink)',
    fontWeight: 500,
  },
  redeemed: {
    border: '1px solid var(--green-ink)',
    background: 'var(--green-tint)',
    color: 'var(--green-ink)',
    fontWeight: 600,
  },
  void: {
    border: '1px solid rgba(158, 27, 27, .4)',
    background: 'var(--paper)',
    color: 'var(--flag-ink)',
    fontWeight: 500,
  },
  expired: { ...PROVISIONAL, background: 'var(--sand-2)', color: 'var(--ink-2)' },
}

function styleFor(props: StatusPillProps): CSSProperties {
  switch (props.kind) {
    case 'request':
      return REQUEST_STYLE[props.status]
    case 'demand':
      return DEMAND_STYLE[props.status]
    case 'opportunity':
      return OPPORTUNITY_STYLE[props.status]
    case 'survey':
      return SURVEY_STYLE[props.status]
    case 'voucher':
      return VOUCHER_STYLE[props.status]
  }
}
