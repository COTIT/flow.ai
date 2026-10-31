import { describe, expect, test } from 'vitest'

import { farmerName, withGrowers } from '@/features/ops/useOpportunity'

const row = (over: Record<string, unknown> = {}) => ({
  harvest_report_id: 'h1',
  crop_cycle_id: 'cy1',
  village_id: 'v1',
  crop_id: 'c1',
  plot_id: 'pl1',
  harvest_start: '2026-09-01',
  harvest_end: '2026-09-30',
  quantity_kg: 12000,
  confidence: null,
  verification: null,
  committed_kg: 6400,
  available_kg: 5600,
  ...over,
})

const plot = (id: string, label: string, people: Array<{ given_name: string; family_name: string }>) => ({
  id,
  label,
  farm: {
    id: `f-${id}`,
    farm_manager: people.map((p, i) => ({ person: { id: `p-${id}-${i}`, ...p } })),
  },
})

describe('farmerName', () => {
  test('the first manager of the farm, given then family name', () => {
    expect(
      farmerName({
        farm_manager: [
          { person: { id: 'p1', given_name: 'Neema', family_name: 'Mwaipopo' } },
          { person: { id: 'p2', given_name: 'Juma', family_name: 'Mwaipopo' } },
        ],
      }),
    ).toEqual({ name: 'Neema Mwaipopo', personId: 'p1' })
  })

  test('no farm, or no manager, is no name', () => {
    expect(farmerName(null)).toEqual({ name: null, personId: null })
    expect(farmerName({ farm_manager: [] })).toEqual({ name: null, personId: null })
    expect(farmerName({ farm_manager: [{ person: null }] })).toEqual({ name: null, personId: null })
  })
})

describe('withGrowers', () => {
  test('each harvest takes its plot label and farmer, joined on plot_id', () => {
    const rows = [row({ harvest_report_id: 'h1', plot_id: 'pl1' }), row({ harvest_report_id: 'h2', plot_id: 'pl2' })]
    const plots = [
      plot('pl2', 'Shamba la mto', [{ given_name: 'Joseph', family_name: 'Kalinga' }]),
      plot('pl1', 'Kipande cha juu', [{ given_name: 'Neema', family_name: 'Mwaipopo' }]),
    ]

    const out = withGrowers(rows, plots)
    expect(out.map((r) => [r.harvest_report_id, r.farmer, r.plot_label])).toEqual([
      ['h1', 'Neema Mwaipopo', 'Kipande cha juu'],
      ['h2', 'Joseph Kalinga', 'Shamba la mto'],
    ])
    // The figures are untouched.
    expect(out[0].available_kg).toBe(5600)
  })

  // Zero rows is an answer: a plot RLS did not return leaves the harvest
  // listed, without names.
  test('a plot that did not come back leaves the harvest listed, unnamed', () => {
    const out = withGrowers([row({ plot_id: 'pl9' })], [])
    expect(out).toHaveLength(1)
    expect(out[0].farmer).toBeNull()
    expect(out[0].plot_label).toBeNull()
  })

  test('a farm with no manager has a plot label but no farmer', () => {
    const out = withGrowers([row()], [plot('pl1', 'Kipande cha juu', [])])
    expect(out[0].plot_label).toBe('Kipande cha juu')
    expect(out[0].farmer).toBeNull()
  })
})
