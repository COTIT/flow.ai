import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const supplyLinksList = vi.fn()
const supplyLinksGrant = vi.fn()
const supplyLinksRevoke = vi.fn()
const supplyChainList = vi.fn()
const supplyChainDispatch = vi.fn()
const farmSupplyList = vi.fn()
const warehousesList = vi.fn()

vi.mock('@genesy/api-client/endpoints/supply-links', () => ({
  supplyLinksList: (...a: unknown[]) => supplyLinksList(...a),
  supplyLinksGrant: (...a: unknown[]) => supplyLinksGrant(...a),
  supplyLinksRevoke: (...a: unknown[]) => supplyLinksRevoke(...a),
}))
vi.mock('@genesy/api-client/endpoints/supply-chain', () => ({
  supplyChainList: (...a: unknown[]) => supplyChainList(...a),
  supplyChainDispatch: (...a: unknown[]) => supplyChainDispatch(...a),
}))
vi.mock('@genesy/api-client/endpoints/farm-supply', () => ({
  farmSupplyList: (...a: unknown[]) => farmSupplyList(...a),
}))
vi.mock('@genesy/api-client/endpoints/warehouses', () => ({
  warehousesList: (...a: unknown[]) => warehousesList(...a),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const hooks = await import('@/features/ops/supply/useSupply')

const page = (data: unknown[], current = 1, last = 1) => ({ data, meta: { current_page: current, last_page: last } })

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>
}

beforeEach(() => vi.clearAllMocks())

describe('links', () => {
  test('lists only the links this workspace granted, every page', async () => {
    supplyLinksList
      .mockResolvedValueOnce(page([{ id: 'l1', direction: 'outgoing' }, { id: 'l2', direction: 'incoming' }], 1, 2))
      .mockResolvedValueOnce(page([{ id: 'l3', direction: 'outgoing' }], 2, 2))
    const { result } = renderHook(() => hooks.useSupplyLinks(), { wrapper: wrapper() })

    await waitFor(() => expect(result.current.data?.map((l) => l.id)).toEqual(['l1', 'l3']))
    expect(supplyLinksList).toHaveBeenCalledWith('w1', { page: 1, page_size: 100 })
    expect(supplyLinksList).toHaveBeenLastCalledWith('w1', { page: 2, page_size: 100 })
  })

  test('grants with the target workspace and the allow-list as the API names them', async () => {
    supplyLinksGrant.mockResolvedValue({ data: { id: 'l9', status: 'pending' } })
    const { result } = renderHook(() => hooks.useGrantLink(), { wrapper: wrapper() })

    await act(() => result.current.mutateAsync({ toWorkspaceId: 'w-shop', fields: ['village'] }))
    expect(supplyLinksGrant).toHaveBeenCalledWith('w1', { to_workspace_id: 'w-shop', fields: ['village'] })
  })

  test('revokes a link by id', async () => {
    supplyLinksRevoke.mockResolvedValue(undefined)
    const { result } = renderHook(() => hooks.useRevokeLink(), { wrapper: wrapper() })

    await act(() => result.current.mutateAsync('l1'))
    expect(supplyLinksRevoke).toHaveBeenCalledWith('w1', 'l1')
  })
})

describe('dispatch', () => {
  test('lists only the transfers this workspace sent', async () => {
    supplyChainList.mockResolvedValue(page([{ id: 't1', direction: 'outgoing' }, { id: 't2', direction: 'incoming' }]))
    const { result } = renderHook(() => hooks.useSupplyTransfers(), { wrapper: wrapper() })

    await waitFor(() => expect(result.current.data?.map((t) => t.id)).toEqual(['t1']))
  })

  test('offers the raw lots of supply batches and the warehouses to send from', async () => {
    farmSupplyList.mockResolvedValue(page([{ id: 'b1', lot: { id: 'lot1', code: 'ILU-H-2026-01', public_code: 'X' } }]))
    warehousesList.mockResolvedValue(page([{ id: 'wh1', name: 'Iringa store' }]))
    const { result } = renderHook(() => hooks.useDispatchOptions(), { wrapper: wrapper() })

    await waitFor(() => expect(result.current.data).toBeDefined())
    expect(result.current.data).toEqual({
      lots: [{ id: 'lot1', code: 'ILU-H-2026-01' }],
      warehouses: [{ id: 'wh1', name: 'Iringa store' }],
    })
  })

  test('sends the quantity as the text typed, never a number', async () => {
    supplyChainDispatch.mockResolvedValue({ data: { id: 't1' } })
    const { result } = renderHook(() => hooks.useDispatch(), { wrapper: wrapper() })

    await act(() => result.current.mutateAsync({ lotId: 'lot1', linkId: 'l1', warehouseId: 'wh1', quantity: '12.50' }))
    expect(supplyChainDispatch).toHaveBeenCalledWith('w1', 'lot1', {
      link_id: 'l1',
      warehouse_id: 'wh1',
      quantity: '12.50',
    })
  })
})
