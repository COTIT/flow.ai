import { describe, expect, test } from 'vitest'

import { CONFIG_TABLES, OBSERVED_TABLES, withProvenance } from '@/lib/provenance'

const ACTOR = '80000000-0000-4000-8000-000000000003'
const AT = '2026-09-10T08:00:00.000Z'

describe('withProvenance', () => {
  test('injects source, captured_at and captured_by', () => {
    const out = withProvenance({ given_name: 'Neema' }, 'field_verified', ACTOR, AT)
    expect(out).toEqual({
      given_name: 'Neema',
      source: 'field_verified',
      captured_at: AT,
      captured_by: ACTOR,
    })
  })

  test('does not mutate the payload it was handed', () => {
    const payload = { quantity_kg: 4100 }
    withProvenance(payload, 'farmer_reported', ACTOR, AT)
    expect(payload).toEqual({ quantity_kg: 4100 })
  })

  // verification is NOT set here. It stays at the column default of
  // 'unverified' and only ever moves through app_verify(), so a record cannot
  // be marked verified without a verifier.
  test('never sets verification, verified_by or verified_at', () => {
    const out = withProvenance({ label: 'Kipande' }, 'field_verified', ACTOR, AT)
    expect(out).not.toHaveProperty('verification')
    expect(out).not.toHaveProperty('verified_by')
    expect(out).not.toHaveProperty('verified_at')
  })

  test('a caller cannot launder captured_by through the payload', () => {
    const out = withProvenance(
      { captured_by: '00000000-0000-4000-8000-000000000000' },
      'farmer_reported',
      ACTOR,
      AT,
    )
    expect(out.captured_by).toBe(ACTOR)
  })

  test('nor override the source it declared', () => {
    const out = withProvenance({ source: 'sensor_derived' }, 'farmer_reported', ACTOR, AT)
    expect(out.source).toBe('farmer_reported')
  })

  test('defaults captured_at to now when not supplied', () => {
    const before = Date.now()
    const out = withProvenance({}, 'farmer_reported', ACTOR)
    const at = Date.parse(out.captured_at)
    expect(at).toBeGreaterThanOrEqual(before)
    expect(at).toBeLessThanOrEqual(Date.now())
  })
})

describe('table classification', () => {
  // Config tables have no provenance columns at all — setting them errors.
  test('the config list matches the schema', () => {
    expect([...CONFIG_TABLES].sort()).toEqual([
      'buyer',
      'country',
      'crop',
      'equipment',
      'equipment_category',
      'project',
      'village',
      'village_capacity',
    ])
  })

  test('the observed list matches the schema', () => {
    expect([...OBSERVED_TABLES].sort()).toEqual([
      'crop_cycle',
      'farm',
      'harvest_report',
      'household',
      'person',
      'plot',
      'pue_request',
    ])
  })

  // buyer_demand is neither: it deliberately has no `source` column, because
  // the five categories classify how a fact about the productive economy was
  // learned and a buyer's requirement is a counterparty input.
  test('buyer_demand belongs to neither list', () => {
    expect(CONFIG_TABLES).not.toContain('buyer_demand')
    expect(OBSERVED_TABLES).not.toContain('buyer_demand')
  })
})
