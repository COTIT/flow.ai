import { describe, expect, test } from 'vitest'

import {
  VERIFIABLE_TABLES,
  countUnverified,
  isVerifiable,
  verifiableRecords,
  type PersonDetail,
} from '@/features/officer/personDetail'

const prov = (verification: PersonDetail['person']['verification']) => ({
  source: 'field_verified' as const,
  verification,
  confidence: 'high' as const,
  captured_at: '2026-09-09T21:30:00Z',
  captured_by: null,
  verified_by: null,
  verified_at: null,
})

const detail = (): PersonDetail => ({
  person: { id: 'p1', given_name: 'Neema', family_name: 'Mwakalinga', phone: null, village_id: 'v1', ...prov('unverified') },
  households: [
    { id: 'h1', label: 'Mwakalinga household', ...prov('verified'), members: [{ id: 'p1', given_name: 'Neema', family_name: 'Mwakalinga' }] },
  ],
  farms: [
    {
      id: 'f1', label: 'Shamba la Neema', latitude: null, longitude: null, ...prov('verified'),
      plots: [
        {
          id: 'pl1', label: 'Kipande cha juu', area_ha: 1.8, latitude: null, longitude: null, ...prov('unverified'),
          cycles: [
            {
              id: 'c1', crop_id: 'crop1', crop_name: 'Maize', season_label: 'Msimu 2026 A',
              area_ha: 1.6, tree_count: null, unit_count: null,
              planted_on: '2026-03-05', harvest_start: '2026-09-01', harvest_end: '2026-09-30',
              status: 'growing', ...prov('pending'),
              harvests: [
                { id: 'hr1', kind: 'expected', quantity_kg: 4100, is_current: true, reported_for: '2026-09-15', ...prov('unverified') },
              ],
            },
          ],
        },
      ],
    },
  ],
})

describe('VERIFIABLE_TABLES', () => {
  // app_verify refuses anything else: "not a verifiable table: %".
  test('matches app_verify exactly', () => {
    expect([...VERIFIABLE_TABLES].sort()).toEqual([
      'crop_cycle',
      'farm',
      'harvest_report',
      'household',
      'person',
      'plot',
    ])
  })

  test('config and market tables are not verifiable', () => {
    expect(isVerifiable('village')).toBe(false)
    expect(isVerifiable('buyer_demand')).toBe(false)
    expect(isVerifiable('pue_request')).toBe(false)
    expect(isVerifiable('person')).toBe(true)
  })
})

describe('verifiableRecords', () => {
  test('walks person, households, farms, plots, cycles and harvests', () => {
    const records = verifiableRecords(detail())
    expect(records.map((r) => `${r.table}:${r.id}`)).toEqual([
      'person:p1',
      'household:h1',
      'farm:f1',
      'plot:pl1',
      'crop_cycle:c1',
      'harvest_report:hr1',
    ])
  })

  test('every record carries the verification it currently has', () => {
    const byId = new Map(verifiableRecords(detail()).map((r) => [r.id, r.verification]))
    expect(byId.get('p1')).toBe('unverified')
    expect(byId.get('h1')).toBe('verified')
    expect(byId.get('c1')).toBe('pending')
  })

  test('an empty detail yields just the person', () => {
    const d = detail()
    d.households = []
    d.farms = []
    expect(verifiableRecords(d)).toHaveLength(1)
  })
})

describe('countUnverified', () => {
  // 'unverified' and 'pending' both still need an officer; 'verified' does
  // not, and 'disputed' is a separate problem rather than pending work.
  test('counts unverified and pending, not verified', () => {
    expect(countUnverified(detail())).toBe(4)
  })

  test('a fully verified record set counts zero', () => {
    const d = detail()
    d.person.verification = 'verified'
    d.farms[0].plots[0].verification = 'verified'
    d.farms[0].plots[0].cycles[0].verification = 'verified'
    d.farms[0].plots[0].cycles[0].harvests[0].verification = 'verified'
    expect(countUnverified(d)).toBe(0)
  })

  test('disputed is not counted as pending work', () => {
    const d = detail()
    d.person.verification = 'disputed'
    d.households[0].verification = 'verified'
    d.farms[0].verification = 'verified'
    d.farms[0].plots[0].verification = 'verified'
    d.farms[0].plots[0].cycles[0].verification = 'verified'
    d.farms[0].plots[0].cycles[0].harvests[0].verification = 'verified'
    expect(countUnverified(d)).toBe(0)
  })
})
