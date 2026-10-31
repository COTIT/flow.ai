import { describe, expect, test } from 'vitest'

import { getVerifyTarget } from '@/features/officer/verifyNavigation'
import type { QueueRow } from '@/features/officer/useVerifyQueue'

const row = (table: QueueRow['table'], extra: Partial<QueueRow> = {}): QueueRow => ({
  table,
  id: '11111111-1111-4111-8111-111111111111',
  label: 'Record',
  village_id: '22222222-2222-4222-8222-222222222222',
  source: 'farmer_reported',
  verification: 'unverified',
  confidence: null,
  captured_at: '2026-09-20T00:00:00Z',
  ...extra,
})

describe('getVerifyTarget', () => {
  test.each([
    ['person', { to: '/officer/people/$personId', params: { personId: '11111111-1111-4111-8111-111111111111' } }],
    ['farm', { to: '/officer/farms/$farmId', params: { farmId: '11111111-1111-4111-8111-111111111111' } }],
    ['crop_cycle', { to: '/officer/cycles/$cycleId', params: { cycleId: '11111111-1111-4111-8111-111111111111' } }],
  ] as const)('%s routes to its own detail', (table, target) => {
    expect(getVerifyTarget(row(table))).toEqual(target)
  })

  test('plot routes to parent farm and focus', () => {
    expect(getVerifyTarget(row('plot', { farm_id: '33333333-3333-4333-8333-333333333333' }))).toEqual({
      to: '/officer/farms/$farmId',
      params: { farmId: '33333333-3333-4333-8333-333333333333' },
      search: { plot: '11111111-1111-4111-8111-111111111111' },
    })
  })

  test('harvest routes to parent cycle and focus', () => {
    expect(getVerifyTarget(row('harvest_report', { crop_cycle_id: '33333333-3333-4333-8333-333333333333' }))).toEqual({
      to: '/officer/cycles/$cycleId',
      params: { cycleId: '33333333-3333-4333-8333-333333333333' },
      search: { harvest: '11111111-1111-4111-8111-111111111111' },
    })
  })

  test('missing parent IDs stay plain text targets', () => {
    expect(getVerifyTarget(row('plot'))).toBeNull()
    expect(getVerifyTarget(row('harvest_report'))).toBeNull()
  })

  test('unknown table is a plain-text fallback', () => {
    expect(getVerifyTarget(row('unknown' as QueueRow['table']))).toBeNull()
  })
})
