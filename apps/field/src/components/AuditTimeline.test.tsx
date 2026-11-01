import { render, screen, within } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { AuditTimeline, type AuditEvent } from '@/components/AuditTimeline'
import '@/i18n'

const EVENTS: AuditEvent[] = [
  { occurred_at: '2026-09-09T13:19:56Z', kind: 'household_registered', actor_name: 'Salima Officer', actor_role: 'field_officer', detail: {} },
  { occurred_at: '2026-09-29T07:00:00Z', kind: 'issued', actor_name: 'Neema Mwakalinga', actor_role: 'farmer', detail: { audit_required: true } },
  { occurred_at: '2026-09-29T08:42:00Z', kind: 'redeemed', actor_name: 'Juma Officer', actor_role: 'field_officer', detail: { id_type_seen: 'nida' } },
  { occurred_at: '2026-09-29T08:40:00Z', kind: 'refused', actor_name: 'Peter Officer', actor_role: 'field_officer', detail: { reason: 'outside your villages' } },
]

describe('AuditTimeline', () => {
  test('names who did each step, in their role, with the time in Tanzania', () => {
    render(<AuditTimeline events={EVENTS} />)
    const redeemed = screen.getByTestId('audit-event-redeemed')
    expect(redeemed).toHaveTextContent('Incentive handed over')
    expect(redeemed).toHaveTextContent('Juma Officer')
    expect(redeemed).toHaveTextContent('Field officer')
    expect(redeemed).toHaveTextContent('29 Sep 2026, 11:42')
  })

  test('says which ID document was checked, and never more than its type', () => {
    render(<AuditTimeline events={EVENTS} />)
    expect(screen.getByTestId('audit-event-redeemed')).toHaveTextContent('ID checked: NIDA card')
  })

  test('shows an audit hold and a refusal reason when the caller may see them', () => {
    render(<AuditTimeline events={EVENTS} />)
    expect(screen.getByTestId('audit-event-issued')).toHaveTextContent('Held for an in-person audit')
    expect(screen.getByTestId('audit-event-refused')).toHaveTextContent('Reason: outside your villages')
  })

  test('keeps the order the database gave, oldest first', () => {
    render(<AuditTimeline events={EVENTS} />)
    const items = within(screen.getByTestId('audit-timeline')).getAllByRole('listitem')
    expect(items.map((li) => li.getAttribute('data-kind'))).toEqual([
      'household_registered',
      'issued',
      'redeemed',
      'refused',
    ])
  })

  test('an empty trail says so rather than rendering nothing', () => {
    render(<AuditTimeline events={[]} />)
    expect(screen.getByTestId('audit-timeline-empty')).toHaveTextContent('Nothing recorded yet.')
  })
})
