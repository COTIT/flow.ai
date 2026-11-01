import { describe, expect, test } from 'vitest'

import { buildEditPayload, selectedEditMeasure, validateEdit } from '@/features/officer/officerEdit'

const valid = { label: '  Shamba  ', confidence: 'high' }

describe('officer edit validation', () => {
  test('selects the active crop measure and preserves fallbacks', () => {
    expect(selectedEditMeasure('farm', {}, { crop: 'tree_count' }, 'area')).toBe('area')
    expect(selectedEditMeasure('crop_cycle', { crop_id: 'crop' }, undefined, 'area')).toBe('area')
    expect(selectedEditMeasure('crop_cycle', { crop_id: 'crop' }, { crop: 'tree_count' }, 'area')).toBe('tree_count')
    expect(selectedEditMeasure('crop_cycle', { crop_id: 'other' }, { crop: 'tree_count' }, 'area')).toBe('area')
    expect(selectedEditMeasure('crop_cycle', {}, {}, 'area')).toBe('area')
  })

  test('requires person and named-record text', () => {
    expect(validateEdit('person', { given_name: '', family_name: 'A' })).toBe('given_name is required')
    expect(validateEdit('person', { given_name: 'A', family_name: ' ' })).toBe('family_name is required')
    expect(validateEdit('household', { label: ' ' })).toBe('label is required')
    expect(validateEdit('household', { label: 'Household' })).toBeNull()
    expect(validateEdit('farm', { label: 'Farm', latitude: '91' })).toContain('latitude')
    expect(validateEdit('plot', { label: 'Plot', area_ha: '-1' })).toContain('area_ha')
  })

  test('validates GPS, area, counts, dates, and quantities', () => {
    expect(validateEdit('farm', { label: 'Farm', latitude: '-90', longitude: '180' })).toBeNull()
    expect(validateEdit('farm', { label: 'Farm', longitude: '-181' })).toContain('longitude')
    expect(validateEdit('plot', { label: 'Plot', area_ha: '1.5' })).toBeNull()
    expect(validateEdit('crop_cycle', { crop_id: 'c', area_ha: '1', harvest_start: '2026-09-02', harvest_end: '2026-09-01' }, 'area')).toContain('harvest window')
    expect(validateEdit('crop_cycle', { crop_id: 'c', tree_count: '1.5' }, 'tree_count')).toContain('whole')
    expect(validateEdit('crop_cycle', { crop_id: 'c', tree_count: '1' }, 'tree_count')).toBeNull()
    expect(validateEdit('crop_cycle', { crop_id: 'c', tree_count: 'abc' }, 'tree_count')).toContain('number')
    expect(validateEdit('crop_cycle', { crop_id: 'c', unit_count: '1' }, undefined)).toBe('crop measure is unavailable')
    expect(validateEdit('harvest_report', { quantity_kg: '-1' })).toContain('quantity_kg')
    expect(validateEdit('harvest_report', { quantity_kg: '2' })).toBeNull()
    expect(validateEdit('crop_cycle', { crop_id: '', area_ha: '1' }, 'area')).toBe('crop_id is required')
    expect(validateEdit('crop_cycle', { crop_id: 'c', unit_count: '2' }, 'unit_count')).toBeNull()
    expect(validateEdit('crop_cycle', { crop_id: 'c', area_ha: '1', harvest_start: '2026-01-01', harvest_end: '2026-01-02' }, 'area')).toBeNull()
  })

  test('builds allowlisted domain payloads and clears optional numbers', () => {
    expect(buildEditPayload('person', { given_name: 'A', family_name: 'B', phone: ' 123 ', confidence: 'high' })).toEqual({ given_name: 'A', family_name: 'B', phone: '123' })
    expect(buildEditPayload('person', { given_name: 'A', family_name: 'B' })).toMatchObject({ phone: '', given_name: 'A' })
    expect(buildEditPayload('household', valid)).toEqual({ label: 'Shamba' })
    expect(buildEditPayload('farm', { ...valid, latitude: '', longitude: '2' })).toEqual({ label: 'Shamba', latitude: null, longitude: 2 })
    expect(buildEditPayload('plot', { ...valid, area_ha: '1', latitude: '3', longitude: '' })).toEqual({ label: 'Shamba', area_ha: 1, latitude: 3, longitude: null })
    expect(buildEditPayload('crop_cycle', { crop_id: 'c', season_label: ' S ', area_ha: '1', tree_count: '', unit_count: '', planted_on: '2026-01-01', harvest_start: '2026-02-01', harvest_end: '2026-03-01', status: 'growing', confidence: 'high' })).toMatchObject({ crop_id: 'c', season_label: 'S', area_ha: 1, tree_count: null, unit_count: null })
    expect(buildEditPayload('crop_cycle', { crop_id: 'c', tree_count: '2', unit_count: '3' })).toMatchObject({ tree_count: 2, unit_count: 3, season_label: '' })
  })

  // The RPC refuses a cycle carrying a measure its crop does not use, so a
  // cycle switched between crops must drop the hidden, stale measure values.
  test('a cycle payload keeps only the measure its crop uses', () => {
    const stale = { crop_id: 'c', area_ha: '2.5', tree_count: '100', unit_count: '7' }
    expect(buildEditPayload('crop_cycle', stale, 'area')).toMatchObject({ area_ha: 2.5, tree_count: null, unit_count: null })
    expect(buildEditPayload('crop_cycle', stale, 'tree_count')).toMatchObject({ area_ha: null, tree_count: 100, unit_count: null })
    expect(buildEditPayload('crop_cycle', stale, 'unit_count')).toMatchObject({ area_ha: null, tree_count: null, unit_count: 7 })
    expect(buildEditPayload('crop_cycle', stale)).toMatchObject({ area_ha: 2.5, tree_count: 100, unit_count: 7 })
    expect(buildEditPayload('farm', { label: 'Farm', latitude: '', longitude: '' }, 'tree_count')).toEqual({ label: 'Farm', latitude: null, longitude: null })
  })

  test('returns no validation error for an unknown runtime table', () => {
    expect(validateEdit('unexpected' as never, {})).toBeNull()
  })
})
