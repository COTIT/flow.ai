import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

const farmRecordsHarvestReportsRevise = vi.fn()
const farmRecordsCropCyclesCorrect = vi.fn()
const farmRecordsPersonsCorrect = vi.fn()
const farmRecordsHouseholdsCorrect = vi.fn()
const farmRecordsFarmsCorrect = vi.fn()
const farmRecordsPlotsCorrect = vi.fn()

vi.mock('@genesy/api-client/endpoints/farm-records', () => ({
  farmRecordsHarvestReportsRevise: (...args: unknown[]) => farmRecordsHarvestReportsRevise(...args),
  farmRecordsCropCyclesCorrect: (...args: unknown[]) => farmRecordsCropCyclesCorrect(...args),
  farmRecordsPersonsCorrect: (...args: unknown[]) => farmRecordsPersonsCorrect(...args),
  farmRecordsHouseholdsCorrect: (...args: unknown[]) => farmRecordsHouseholdsCorrect(...args),
  farmRecordsFarmsCorrect: (...args: unknown[]) => farmRecordsFarmsCorrect(...args),
  farmRecordsPlotsCorrect: (...args: unknown[]) => farmRecordsPlotsCorrect(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { useOfficerEdit } = await import('@/features/officer/useOfficerEdit')

function renderEdit(measure?: 'area' | 'tree_count' | 'unit_count') {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  client.invalidateQueries = vi.fn().mockResolvedValue(undefined)
  const hook = renderHook(() => useOfficerEdit(measure), {
    wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
  })
  return { ...hook, client }
}

describe('useOfficerEdit', () => {
  // PUT /farm/{kind}/{id} with every editable field; GPS and area travel as decimal text.
  test('corrects a farm through the API and invalidates related surfaces', async () => {
    farmRecordsFarmsCorrect.mockResolvedValue({ data: {} })
    const { result, client } = renderEdit()
    await result.current.mutateAsync({
      table: 'farm',
      id: 'f1',
      values: { label: ' Farm ', latitude: '', longitude: ' 35.710001 ' },
      context: { villageId: 'v1', personId: 'p1', farmId: 'f1' },
    })
    expect(farmRecordsFarmsCorrect).toHaveBeenCalledWith('w1', 'f1', { label: 'Farm', latitude: null, longitude: '35.710001' })
    expect(client.invalidateQueries).toHaveBeenCalled()
  })

  test('corrects a person, a plot and a household through the API', async () => {
    farmRecordsPersonsCorrect.mockResolvedValue({ data: {} })
    farmRecordsPlotsCorrect.mockResolvedValue({ data: {} })
    farmRecordsHouseholdsCorrect.mockResolvedValue({ data: {} })
    const { result } = renderEdit()
    await result.current.mutateAsync({
      table: 'person', id: 'p1', values: { given_name: ' Asha ', family_name: 'Mwakalinga', phone: ' ' }, context: { villageId: 'v1' },
    })
    expect(farmRecordsPersonsCorrect).toHaveBeenCalledWith('w1', 'p1', { given_name: 'Asha', family_name: 'Mwakalinga', phone: null })
    await result.current.mutateAsync({
      table: 'person', id: 'p1', values: { given_name: 'Asha', family_name: 'Mwakalinga' }, context: { villageId: 'v1' },
    })
    expect(farmRecordsPersonsCorrect).toHaveBeenLastCalledWith('w1', 'p1', { given_name: 'Asha', family_name: 'Mwakalinga', phone: null })
    await result.current.mutateAsync({
      table: 'plot', id: 'pl1', values: { label: 'Riverside', area_ha: '1.80', latitude: '-7.7', longitude: '' }, context: { villageId: 'v1' },
    })
    expect(farmRecordsPlotsCorrect).toHaveBeenCalledWith('w1', 'pl1', { label: 'Riverside', area_ha: '1.80', latitude: '-7.7', longitude: null })
    await result.current.mutateAsync({
      table: 'household', id: 'h1', values: { label: ' Home ' }, context: { villageId: 'v1' },
    })
    expect(farmRecordsHouseholdsCorrect).toHaveBeenCalledWith('w1', 'h1', { label: 'Home' })
  })

  // Kilograms travel as decimal text, never through a float.
  test('supersedes current harvest and preserves context', async () => {
    farmRecordsHarvestReportsRevise.mockResolvedValue({ data: { id: 'new-harvest' } })
    const { result } = renderEdit()
    await result.current.mutateAsync({
      table: 'harvest_report',
      id: 'old-harvest',
      values: { quantity_kg: ' 12.5 ', reported_for: '', confidence: 'medium' },
      context: { villageId: 'v1', cycleId: 'c1', harvestKind: 'expected' },
      clientRef: '0199a000-0000-7000-8000-000000000001',
    })
    // The draft's reference: a resend after a lost reply files the figure once.
    expect(farmRecordsHarvestReportsRevise).toHaveBeenCalledWith('w1', 'c1', {
      kind: 'expected',
      quantity_kg: '12.5',
      source: 'field_verified',
      confidence: 'medium',
      reported_for: null,
      client_ref: '0199a000-0000-7000-8000-000000000001',
    })
    expect(farmRecordsFarmsCorrect).not.toHaveBeenCalled()
  })

  test('a supersede with a date and no confidence sends the date and null', async () => {
    farmRecordsHarvestReportsRevise.mockResolvedValue({ data: { id: 'new-harvest' } })
    const { result } = renderEdit()
    await result.current.mutateAsync({
      table: 'harvest_report',
      id: 'old-harvest',
      values: { quantity_kg: '4100', reported_for: '2026-10-01' },
      context: { villageId: 'v1', cycleId: 'c1', harvestKind: 'actual' },
    })
    expect(farmRecordsHarvestReportsRevise).toHaveBeenCalledWith('w1', 'c1', expect.objectContaining({
      quantity_kg: '4100',
      confidence: null,
      reported_for: '2026-10-01',
    }))
  })

  test('validates a cycle against the selected crop measure', async () => {
    const { result } = renderEdit('area')
    await result.current.mutateAsync({
      table: 'crop_cycle',
      id: 'c1',
      values: { crop_id: 'tree-crop', tree_count: '2' },
      measure: 'tree_count',
      context: { villageId: 'v1', cycleId: 'c1' },
    })
    expect(farmRecordsCropCyclesCorrect).toHaveBeenCalledWith('w1', 'c1', expect.objectContaining({ crop_id: 'tree-crop' }))
  })

  // PUT needs every editable field; the area travels as decimal text.
  test('an area cycle is corrected with every field and the area as text', async () => {
    farmRecordsCropCyclesCorrect.mockReset()
    farmRecordsCropCyclesCorrect.mockResolvedValue({ data: {} })
    const { result } = renderEdit('area')
    await result.current.mutateAsync({
      table: 'crop_cycle',
      id: 'c1',
      values: {
        crop_id: 'maize', season_label: ' Msimu A ', area_ha: ' 1.60 ', tree_count: '', unit_count: '',
        planted_on: '2026-03-01', harvest_start: '', harvest_end: '', status: 'growing',
      },
      context: { villageId: 'v1', cycleId: 'c1' },
    })
    expect(farmRecordsCropCyclesCorrect).toHaveBeenCalledWith('w1', 'c1', {
      crop_id: 'maize',
      season_label: 'Msimu A',
      area_ha: '1.60',
      tree_count: null,
      unit_count: null,
      planted_on: '2026-03-01',
      harvest_start: '',
      harvest_end: '',
      status: 'growing',
    })
  })

  test('a cycle switched from an area crop to a tree crop sends only the tree count', async () => {
    farmRecordsCropCyclesCorrect.mockReset()
    farmRecordsCropCyclesCorrect.mockResolvedValue({ data: {} })
    const { result } = renderEdit('area')
    await result.current.mutateAsync({
      table: 'crop_cycle',
      id: 'c1',
      values: { crop_id: 'tree-crop', area_ha: '2.5', tree_count: '100', unit_count: '' },
      measure: 'tree_count',
      context: { villageId: 'v1', cycleId: 'c1' },
    })
    expect(farmRecordsCropCyclesCorrect).toHaveBeenCalledWith('w1', 'c1', expect.objectContaining({
      area_ha: null, tree_count: 100, unit_count: null,
    }))
  })

  test('a cycle falls back to the hook measure when none is selected', async () => {
    farmRecordsCropCyclesCorrect.mockReset()
    farmRecordsCropCyclesCorrect.mockResolvedValue({ data: {} })
    const { result } = renderEdit('unit_count')
    await result.current.mutateAsync({
      table: 'crop_cycle',
      id: 'c1',
      values: { crop_id: 'unit-crop', area_ha: '2.5', unit_count: '4' },
      context: { villageId: 'v1', cycleId: 'c1' },
    })
    expect(farmRecordsCropCyclesCorrect).toHaveBeenCalledWith('w1', 'c1', expect.objectContaining({
      area_ha: null, tree_count: null, unit_count: 4,
    }))
  })

  test('rejects validation, missing harvest context, and server failures', async () => {
    const { result } = renderEdit('area')
    await expect(result.current.mutateAsync({
      table: 'farm', id: 'f1', values: { label: ' ' }, context: { villageId: 'v1' },
    })).rejects.toThrow('label is required')
    await expect(result.current.mutateAsync({
      table: 'harvest_report', id: 'h1', values: { quantity_kg: '1' }, context: { villageId: 'v1' },
    })).rejects.toThrow('harvest context is required')

    const forbidden = { response: { status: 403, data: { message: 'forbidden' } } }
    farmRecordsFarmsCorrect.mockRejectedValue(forbidden)
    await expect(result.current.mutateAsync({
      table: 'farm', id: 'f1', values: { label: 'Farm' }, context: { villageId: 'v1' },
    })).rejects.toBe(forbidden)
    const refused = { response: { status: 422, data: { message: 'harvest update failed' } } }
    farmRecordsHarvestReportsRevise.mockRejectedValue(refused)
    await expect(result.current.mutateAsync({
      table: 'harvest_report', id: 'h1', values: { quantity_kg: '1' }, context: { villageId: 'v1', cycleId: 'c1', harvestKind: 'actual' },
    })).rejects.toBe(refused)
    await waitFor(() => expect(result.current.isError).toBe(true))
  })

  test('supports a context with no optional invalidation IDs', async () => {
    farmRecordsHouseholdsCorrect.mockResolvedValue({ data: {} })
    const { result } = renderEdit()
    await result.current.mutateAsync({
      table: 'household', id: 'h1', values: { label: 'Household' }, context: { villageId: '' },
    })
  })
})
