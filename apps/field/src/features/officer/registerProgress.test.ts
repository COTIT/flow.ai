import { describe, expect, test } from 'vitest'

import type { CycleForm, RegisterForm } from '@/features/officer/registerPayload'
import {
  REGISTER_GROUPS,
  completedGroups,
  isGroupComplete,
} from '@/features/officer/registerProgress'

const EMPTY: RegisterForm = {
  given_name: '',
  family_name: '',
  phone: '',
  household_label: '',
  is_head: true,
  farm_label: '',
  farm_latitude: '',
  farm_longitude: '',
  plot_label: '',
  plot_area_ha: '',
  season_label: '',
  planted_on: '',
  cycles: [],
  confidence: 'medium',
}

const MEASURES = { maize: 'area', avocado: 'tree_count', honey: 'unit_count' } as const

const cycle = (over: Partial<CycleForm> = {}): CycleForm => ({
  crop_id: 'maize',
  area_ha: '',
  tree_count: '',
  unit_count: '',
  harvest_start: '2026-09-01',
  harvest_end: '2026-09-30',
  harvest_quantity_kg: '',
  ...over,
})

/**
 * The completion rail. Six chips, one per group the RPC creates, so the officer
 * can see what is done and what is left without the form being split into six
 * submits — it is still one page, one submit, one transaction.
 */
describe('which groups are filled in', () => {
  test('nothing typed, nothing complete', () => {
    expect(completedGroups(EMPTY, MEASURES)).toEqual([])
  })

  test('the groups are the six the RPC creates, in the order it creates them', () => {
    expect(REGISTER_GROUPS).toEqual(['person', 'household', 'farm', 'plot', 'cycle', 'harvest'])
  })

  test('a person needs both names; a phone alone does not count', () => {
    expect(isGroupComplete('person', { ...EMPTY, given_name: 'Amina' }, MEASURES)).toBe(false)
    expect(
      isGroupComplete('person', { ...EMPTY, given_name: 'Amina', family_name: 'Sanga' }, MEASURES),
    ).toBe(true)
    expect(isGroupComplete('person', { ...EMPTY, phone: '+255700000101' }, MEASURES)).toBe(false)
  })

  test('whitespace is not an answer', () => {
    expect(
      isGroupComplete('person', { ...EMPTY, given_name: '  ', family_name: '  ' }, MEASURES),
    ).toBe(false)
  })

  test('a plot needs its area as well as its name', () => {
    expect(isGroupComplete('plot', { ...EMPTY, plot_label: 'Kipande' }, MEASURES)).toBe(false)
    expect(
      isGroupComplete('plot', { ...EMPTY, plot_label: 'Kipande', plot_area_ha: '0.6' }, MEASURES),
    ).toBe(true)
  })

  // Each crop decides which measure field it asks for, so the rail asks for
  // the same one the form shows under that crop.
  test('each crop asks for the measure it uses, and no other', () => {
    const done = (c: CycleForm) => isGroupComplete('cycle', { ...EMPTY, cycles: [c] }, MEASURES)

    expect(done(cycle({ area_ha: '0.6' }))).toBe(true)
    expect(done(cycle({ tree_count: '40' }))).toBe(false)
    expect(done(cycle({ crop_id: 'avocado', tree_count: '40' }))).toBe(true)
    expect(done(cycle({ crop_id: 'honey', unit_count: '12' }))).toBe(true)
  })

  test('with no crop chosen the crops group is open', () => {
    expect(isGroupComplete('cycle', EMPTY, MEASURES)).toBe(false)
  })

  test('every crop chosen has to be filled in, not just the first', () => {
    const form = { ...EMPTY, cycles: [cycle({ area_ha: '0.6' }), cycle({ crop_id: 'avocado' })] }
    expect(isGroupComplete('cycle', form, MEASURES)).toBe(false)
  })

  test('the harvest group needs a figure for every crop', () => {
    const one = { ...EMPTY, cycles: [cycle({ harvest_quantity_kg: '900' }), cycle({ crop_id: 'avocado' })] }
    expect(isGroupComplete('harvest', one, MEASURES)).toBe(false)
    const both = {
      ...EMPTY,
      cycles: [cycle({ harvest_quantity_kg: '900' }), cycle({ crop_id: 'avocado', harvest_quantity_kg: '200' })],
    }
    expect(isGroupComplete('harvest', both, MEASURES)).toBe(true)
  })

  test('a filled form completes all six', () => {
    const full: RegisterForm = {
      ...EMPTY,
      given_name: 'Amina',
      family_name: 'Sanga',
      household_label: 'Kaya ya Sanga',
      farm_label: 'Shamba la Sanga',
      plot_label: 'Kipande kimoja',
      plot_area_ha: '0.6',
      cycles: [cycle({ area_ha: '0.6', harvest_quantity_kg: '1450' })],
    }

    expect(completedGroups(full, MEASURES)).toEqual([...REGISTER_GROUPS])
  })
})
