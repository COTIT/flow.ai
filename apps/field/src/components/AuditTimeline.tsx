import { useTranslation } from 'react-i18next'

import { formatTimestamp } from '@/lib/format'

/** One row of `app_voucher_timeline`, already filtered for the caller. */
export interface AuditEvent {
  occurred_at: string
  kind: string
  actor_name: string | null
  actor_role: string | null
  detail: Record<string, unknown> | null
}

/**
 * Who did what, and when — names, not ids. The database decides what each
 * caller may see (a farmer never sees scans or the audit hold); this only
 * renders it. Order is the database's: oldest first.
 */
export function AuditTimeline({ events }: { events: AuditEvent[] }) {
  const { t } = useTranslation()

  if (events.length === 0) {
    return (
      <p data-testid="audit-timeline-empty" className="type-note" style={{ color: 'var(--ink-3)' }}>
        {t('auditTrail.empty')}
      </p>
    )
  }

  return (
    <ol data-testid="audit-timeline" className="flex flex-col">
      {events.map((event, index) => (
        <li
          key={`${event.kind}-${event.occurred_at}-${index}`}
          data-testid={`audit-event-${event.kind}`}
          data-kind={event.kind}
          className="flex flex-col gap-0.5 py-2.5 pl-3"
          style={{ borderLeft: '2px solid var(--rule-2)' }}
        >
          <span className="type-body-strong">
            {t(`auditTrail.kind.${event.kind}`, { defaultValue: event.kind })}
          </span>
          <span className="type-note" style={{ color: 'var(--ink-2)' }}>
            {[
              event.actor_name,
              event.actor_role &&
                t(`auditTrail.role.${event.actor_role}`, { defaultValue: event.actor_role }),
              formatTimestamp(event.occurred_at),
            ]
              .filter(Boolean)
              .join(' · ')}
          </span>
          {details(event.detail).map((line) => (
            <span key={line.key} className="type-note" style={{ color: 'var(--ink-3)' }}>
              {line.key === 'id'
                ? t('auditTrail.idSeen', {
                    type: t(`auditTrail.idType.${line.value}`, { defaultValue: line.value }),
                  })
                : line.key === 'held'
                  ? t('auditTrail.heldForAudit')
                  : line.key === 'blocked'
                    ? t('auditTrail.blocked', { reason: line.value })
                    : t('auditTrail.reason', { reason: line.value })}
            </span>
          ))}
        </li>
      ))}
    </ol>
  )
}

type DetailLine = { key: 'id' | 'held' | 'reason' | 'blocked'; value: string }

function details(detail: Record<string, unknown> | null): DetailLine[] {
  if (!detail) return []
  const lines: DetailLine[] = []
  if (typeof detail.id_type_seen === 'string') lines.push({ key: 'id', value: detail.id_type_seen })
  if (detail.audit_required === true || detail.needs_ops === true)
    lines.push({ key: 'held', value: '' })
  if (typeof detail.reason === 'string') lines.push({ key: 'reason', value: detail.reason })
  if (typeof detail.blocked_reason === 'string')
    lines.push({ key: 'blocked', value: detail.blocked_reason })
  return lines
}
