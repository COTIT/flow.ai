import { describe, expect, test } from 'vitest'

import {
  buildRegisterPayload,
  type CycleForm,
  type RegisterForm,
} from '@/features/officer/registerPayload'

const VILLAGE = '30000000-0000-4000-8000-000000000001'
const CLIENT_REF = 'c0ffee00-0000-4000-8000-000000000001'
const MAIZE = '40000000-0000-4000-8000-000000000001'
const COFFEE = '40000000-0000-4000-8000-000000000003'
const HONEY = '40000000-0000-4000-8000-000000000005'
const MEASURES = { [MAIZE]: 'area', [COFFEE]: 'tree_count', [HONEY]: 'unit_count' } as const

const cycle = (over: Partial<CycleForm> = {}): CycleForm => ({
  crop_id: MAIZE,
  area_ha: '1.2',
  tree_count: '',
  unit_count: '',
  harvest_start: '2026-09-01',
  harvest_end: '2026-09-30',
  harvest_quantity_kg: '3000',
  ...over,
})

const form = (over: Partial<RegisterForm> = {}): RegisterForm => ({
  given_name: 'Test',
  family_name: 'E2E-abc',
  phone: '',
  household_label: '',
  is_head: true,
  farm_label: 'E2E-abc farm',
  farm_latitude: '',
  farm_longitude: '',
  plot_label: 'E2E-abc plot',
  plot_area_ha: '1.5',
  season_label: '',
  planted_on: '',
  cycles: [cycle()],
  confidence: 'high',
  ...over,
})

const build = (over: Partial<RegisterForm> = {}) =>
  buildRegisterPayload(form(over), { clientRef: CLIENT_REF, villageId: VILLAGE, measures: MEASURES })

describe('buildRegisterPayload', () => {
  test('carries the client_ref and village at the top level', () => {
    const p = build()
    expect(p.client_ref).toBe(CLIENT_REF)
    expect(p.village_id).toBe(VILLAGE)
  })

  test('never sends provenance: the RPC stamps field_verified server-side', () => {
    const p = build()
    expect(p).not.toHaveProperty('source')
    expect(p.person).not.toHaveProperty('source')
    expect(p.person).not.toHaveProperty('captured_by')
    expect(p.farm).not.toHaveProperty('source')
    expect(p.cycles[0]).not.toHaveProperty('source')
  })

  test('an omitted phone is sent as empty, which the RPC nullifs', () => {
    expect(build({ phone: '' }).person.phone).toBe('')
    expect(build({ phone: '+255700000999' }).person.phone).toBe('+255700000999')
  })

  test('an omitted household label lets the RPC derive one from the family name', () => {
    expect(build({ household_label: '' }).household.label).toBe('')
    expect(build({ household_label: 'Mine' }).household.label).toBe('Mine')
  })

  test('is_head is carried through', () => {
    expect(build({ is_head: false }).household.is_head).toBe(false)
  })

  test('sends one cycle per crop chosen, never the old single cycle', () => {
    const p = build({ cycles: [cycle(), cycle({ crop_id: COFFEE, area_ha: '', tree_count: '40' })] })
    expect(p.cycles.map((c) => c.crop_id)).toEqual([MAIZE, COFFEE])
    expect(p).not.toHaveProperty('cycle')
    expect(p).not.toHaveProperty('harvest')
  })

  // The RPC raises 'this crop is measured by area: area_ha is required' and
  // its siblings. Each crop sends only the measure it is measured by.
  test('each crop sends its own measure and no other', () => {
    const p = build({
      cycles: [
        cycle({ area_ha: '1.2', tree_count: '99', unit_count: '7' }),
        cycle({ crop_id: COFFEE, area_ha: '1.2', tree_count: '40' }),
        cycle({ crop_id: HONEY, area_ha: '1.2', unit_count: '24' }),
      ],
    })
    expect(p.cycles[0]).toMatchObject({ area_ha: '1.2', tree_count: '', unit_count: '' })
    expect(p.cycles[1]).toMatchObject({ area_ha: '', tree_count: '40', unit_count: '' })
    expect(p.cycles[2]).toMatchObject({ area_ha: '', tree_count: '', unit_count: '24' })
  })

  test('each crop carries its own expected harvest, reported for its own window', () => {
    const p = build({
      cycles: [
        cycle({ harvest_quantity_kg: '3000' }),
        cycle({ crop_id: COFFEE, tree_count: '40', harvest_start: '2027-01-10', harvest_quantity_kg: '600' }),
      ],
    })
    expect(p.cycles[0].harvest).toMatchObject({ quantity_kg: '3000', reported_for: '2026-09-01' })
    expect(p.cycles[1].harvest).toMatchObject({ quantity_kg: '600', reported_for: '2027-01-10' })
  })

  // An expected harvest is optional: a cycle can be registered without one.
  test('an omitted harvest quantity is sent empty so the RPC skips the report', () => {
    expect(build({ cycles: [cycle({ harvest_quantity_kg: '' })] }).cycles[0].harvest.quantity_kg).toBe('')
  })

  test('dates pass through untouched, never timezone-converted', () => {
    const p = build({ planted_on: '2026-03-05' })
    expect(p.cycles[0].harvest_start).toBe('2026-09-01')
    expect(p.cycles[0].harvest_end).toBe('2026-09-30')
    expect(p.cycles[0].planted_on).toBe('2026-03-05')
  })

  test('the cycle status defaults to growing, matching the RPC default', () => {
    expect(build().cycles[0].status).toBe('growing')
  })

  test('confidence is carried on each observed record', () => {
    const p = build({ confidence: 'medium' })
    expect(p.person.confidence).toBe('medium')
    expect(p.farm.confidence).toBe('medium')
    expect(p.plot.confidence).toBe('medium')
    expect(p.cycles[0].confidence).toBe('medium')
    expect(p.cycles[0].harvest.confidence).toBe('medium')
  })

  test('GPS is sent as typed', () => {
    const p = build({ farm_latitude: '-8.1301', farm_longitude: '35.1892' })
    expect(p.farm.latitude).toBe('-8.1301')
    expect(p.farm.longitude).toBe('35.1892')
  })

  // A crop the list no longer knows has no measure to send. Better to stop
  // than to send a cycle the RPC is certain to refuse.
  test('refuses a crop whose measure is unknown', () => {
    expect(() => build({ cycles: [cycle({ crop_id: 'gone' })] })).toThrow()
  })
})
