import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

const rpc = vi.fn()

vi.mock('@/lib/supabase', () => ({ supabase: { rpc } }))

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
  test('updates a domain record and invalidates related surfaces', async () => {
    rpc.mockResolvedValue({ data: null, error: null })
    const { result, client } = renderEdit()
    await result.current.mutateAsync({
      table: 'farm',
      id: 'f1',
      values: { label: 'Farm', latitude: '', longitude: '2' },
      context: { villageId: 'v1', personId: 'p1', farmId: 'f1' },
    })
    expect(rpc).toHaveBeenCalledWith('app_update_observed_record', {
      p_table: 'farm',
      p_id: 'f1',
      p_payload: { label: 'Farm', latitude: null, longitude: 2 },
    })
    expect(client.invalidateQueries).toHaveBeenCalled()
  })

  test('supersedes current harvest and preserves context', async () => {
    rpc.mockResolvedValue({ data: 'new-harvest', error: null })
    const { result } = renderEdit()
    await result.current.mutateAsync({
      table: 'harvest_report',
      id: 'old-harvest',
      values: { quantity_kg: '12.5', reported_for: '', confidence: 'medium' },
      context: { villageId: 'v1', cycleId: 'c1', harvestKind: 'expected' },
    })
    expect(rpc).toHaveBeenCalledWith('app_supersede_harvest', {
      p_cycle: 'c1',
      p_kind: 'expected',
      p_quantity_kg: 12.5,
      p_source: 'field_verified',
      p_confidence: 'medium',
      p_reported_for: undefined,
    })
  })

  test('validates a cycle against the selected crop measure', async () => {
    rpc.mockResolvedValue({ data: null, error: null })
    const { result } = renderEdit('area')
    await result.current.mutateAsync({
      table: 'crop_cycle',
      id: 'c1',
      values: { crop_id: 'tree-crop', tree_count: '2' },
      measure: 'tree_count',
      context: { villageId: 'v1', cycleId: 'c1' },
    })
    expect(rpc).toHaveBeenCalledWith('app_update_observed_record', expect.objectContaining({ p_table: 'crop_cycle', p_id: 'c1' }))
  })

  test('a cycle switched from an area crop to a tree crop sends only the tree count', async () => {
    rpc.mockReset()
    rpc.mockResolvedValue({ data: null, error: null })
    const { result } = renderEdit('area')
    await result.current.mutateAsync({
      table: 'crop_cycle',
      id: 'c1',
      values: { crop_id: 'tree-crop', area_ha: '2.5', tree_count: '100', unit_count: '' },
      measure: 'tree_count',
      context: { villageId: 'v1', cycleId: 'c1' },
    })
    expect(rpc).toHaveBeenCalledWith('app_update_observed_record', expect.objectContaining({
      p_payload: expect.objectContaining({ area_ha: null, tree_count: 100, unit_count: null }),
    }))
  })

  test('a cycle falls back to the hook measure when none is selected', async () => {
    rpc.mockReset()
    rpc.mockResolvedValue({ data: null, error: null })
    const { result } = renderEdit('unit_count')
    await result.current.mutateAsync({
      table: 'crop_cycle',
      id: 'c1',
      values: { crop_id: 'unit-crop', area_ha: '2.5', unit_count: '4' },
      context: { villageId: 'v1', cycleId: 'c1' },
    })
    expect(rpc).toHaveBeenCalledWith('app_update_observed_record', expect.objectContaining({
      p_payload: expect.objectContaining({ area_ha: null, tree_count: null, unit_count: 4 }),
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

    rpc.mockResolvedValue({ error: { message: 'database is unavailable' } })
    await expect(result.current.mutateAsync({
      table: 'farm', id: 'f1', values: { label: 'Farm' }, context: { villageId: 'v1' },
    })).rejects.toThrow('database is unavailable')
    rpc.mockResolvedValue({ error: { message: 'harvest update failed' } })
    await expect(result.current.mutateAsync({
      table: 'harvest_report', id: 'h1', values: { quantity_kg: '1' }, context: { villageId: 'v1', cycleId: 'c1', harvestKind: 'actual' },
    })).rejects.toThrow('harvest update failed')
    await waitFor(() => expect(result.current.isError).toBe(true))
  })

  test('supports a context with no optional invalidation IDs', async () => {
    rpc.mockResolvedValue({ data: null, error: null })
    const { result } = renderEdit()
    await result.current.mutateAsync({
      table: 'household', id: 'h1', values: { label: 'Household' }, context: { villageId: '' },
    })
  })
})
