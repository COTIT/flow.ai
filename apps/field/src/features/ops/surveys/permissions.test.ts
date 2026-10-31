import { describe, expect, test } from 'vitest'

import { canAuthorSurveys } from '@/features/ops/surveys/permissions'

const membership = (over: Record<string, unknown> = {}) => ({
  id: 'm1',
  role: 'admin' as const,
  project_id: '20000000-0000-4000-8000-000000000001',
  village_id: null,
  revoked_at: null as string | null,
  ...over,
})

/**
 * Whether to RENDER authoring controls. survey_guard and the survey policies
 * are the boundary; this only keeps controls off a screen whose writes the
 * database would refuse.
 */
describe('canAuthorSurveys', () => {
  test('an active admin membership may author', () => {
    expect(canAuthorSurveys([membership()])).toBe(true)
  })

  test('ops may not — they read results and void vouchers only', () => {
    expect(canAuthorSurveys([membership({ role: 'ops' })])).toBe(false)
  })

  test('a revoked admin membership does not count', () => {
    expect(canAuthorSurveys([membership({ revoked_at: '2026-09-01T00:00:00Z' })])).toBe(false)
  })

  test('no memberships, or none loaded yet, is no authoring', () => {
    expect(canAuthorSurveys([])).toBe(false)
    expect(canAuthorSurveys(undefined)).toBe(false)
  })

  test('an admin who is also ops may author', () => {
    expect(canAuthorSurveys([membership({ role: 'ops', id: 'm2' }), membership()])).toBe(true)
  })

  test('field officer and farmer never author', () => {
    expect(canAuthorSurveys([membership({ role: 'field_officer' }), membership({ role: 'farmer' })])).toBe(false)
  })
})
