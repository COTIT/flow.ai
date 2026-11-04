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
const farmSupplyDispatchOptions = vi.fn()
const farmSupplyOptions = vi.fn()
const farmSupplyRawProduct = vi.fn()
const farmSupplyCreate = vi.fn()
const warehousesList = vi.fn()
const lotsStock = vi.fn()

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
  farmSupplyDispatchOptions: (...a: unknown[]) => farmSupplyDispatchOptions(...a),
  farmSupplyOptions: (...a: unknown[]) => farmSupplyOptions(...a),
  farmSupplyRawProduct: (...a: unknown[]) => farmSupplyRawProduct(...a),
  farmSupplyCreate: (...a: unknown[]) => farmSupplyCreate(...a),
}))
vi.mock('@genesy/api-client/endpoints/warehouses', () => ({
  warehousesList: (...a: unknown[]) => warehousesList(...a),
}))
vi.mock('@genesy/api-client/endpoints/lots', () => ({
  lotsStock: (...a: unknown[]) => lotsStock(...a),
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

  test('grants with the target workspace and the allow-list as the API names them, under the form\'s Idempotency-Key', async () => {
    supplyLinksGrant.mockResolvedValue({ data: { id: 'l9', status: 'pending' } })
    const { result } = renderHook(() => hooks.useGrantLink(), { wrapper: wrapper() })

    await act(() => result.current.mutateAsync({ toWorkspaceId: 'w-shop', fields: ['village'], idempotencyKey: 'k-1' }))
    expect(supplyLinksGrant).toHaveBeenCalledWith(
      'w1',
      { to_workspace_id: 'w-shop', fields: ['village'] },
      { headers: { 'Idempotency-Key': 'k-1' } },
    )
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

  test('reads links, lots and warehouses from the farm dispatch options, never the office lists', async () => {
    const options = {
      links: [{ id: 'l1', partner: { id: 'w2', name: 'AFM shop' } }],
      lots: [{ id: 'lot1', code: 'ILU-H-2026-01' }],
      warehouses: [{ id: 'wh1', name: 'Iringa store' }],
    }
    farmSupplyDispatchOptions.mockResolvedValue({ data: options })
    const { result } = renderHook(() => hooks.useDispatchOptions(), { wrapper: wrapper() })

    await waitFor(() => expect(result.current.data).toBeDefined())
    expect(result.current.data).toEqual(options)
    expect(farmSupplyDispatchOptions).toHaveBeenCalledWith('w1')
    expect(supplyLinksList).not.toHaveBeenCalled()
    expect(warehousesList).not.toHaveBeenCalled()
  })

  test('sends the quantity as the text typed, never a number', async () => {
    supplyChainDispatch.mockResolvedValue({ data: { id: 't1' } })
    const { result } = renderHook(() => hooks.useDispatch(), { wrapper: wrapper() })

    await act(() =>
      result.current.mutateAsync({ lotId: 'lot1', linkId: 'l1', warehouseId: 'wh1', quantity: '12.50', idempotencyKey: 'k-2' }),
    )
    expect(supplyChainDispatch).toHaveBeenCalledWith(
      'w1',
      'lot1',
      { link_id: 'l1', warehouse_id: 'wh1', quantity: '12.50' },
      { headers: { 'Idempotency-Key': 'k-2' } },
    )
  })

  test("reads the picked lot's stock per warehouse, and nothing before a lot is picked", async () => {
    const rows = [{ warehouse_id: 'wh1', level: '40.00', reserved: '5.00', available: '35.00' }]
    lotsStock.mockResolvedValue({ data: rows })
    const { result, rerender } = renderHook(({ lot }) => hooks.useLotStock(lot), {
      wrapper: wrapper(),
      initialProps: { lot: '' },
    })
    expect(lotsStock).not.toHaveBeenCalled()

    rerender({ lot: 'lot1' })
    await waitFor(() => expect(result.current.data).toEqual(rows))
    expect(lotsStock).toHaveBeenCalledWith('w1', 'lot1')
  })
})

describe('buying harvest into a batch', () => {
  const options = { harvests: [], raw_products: [], can_create_product: true }

  test('reads the options only once a village and a crop are chosen', async () => {
    farmSupplyOptions.mockResolvedValue({ data: options })
    const none = renderHook(() => hooks.useBatchOptions('v1', ''), { wrapper: wrapper() })
    await waitFor(() => expect(none.result.current.fetchStatus).toBe('idle'))
    expect(farmSupplyOptions).not.toHaveBeenCalled()

    const both = renderHook(() => hooks.useBatchOptions('v1', 'crop1'), { wrapper: wrapper() })
    await waitFor(() => expect(both.result.current.data).toEqual(options))
    expect(farmSupplyOptions).toHaveBeenCalledWith('w1', { village_id: 'v1', crop_id: 'crop1' })
  })

  test('a raw product goes to the farm route with only its name', async () => {
    farmSupplyRawProduct.mockResolvedValue({ data: { id: 'p9', name: 'Raw maize', unit: 'kg' } })
    const { result } = renderHook(() => hooks.useCreateRawProduct(), { wrapper: wrapper() })
    let made: unknown
    await act(async () => {
      made = await result.current.mutateAsync('Raw maize')
    })
    expect(farmSupplyRawProduct).toHaveBeenCalledWith('w1', { name: 'Raw maize' })
    expect(made).toEqual({ id: 'p9', name: 'Raw maize', unit: 'kg' })
  })

  test('a batch is posted as built, its client_ref the retry key', async () => {
    farmSupplyCreate.mockResolvedValue({ data: { id: 'b9' } })
    const body = {
      client_ref: 'c1',
      village_id: 'v1',
      crop_id: 'crop1',
      code: 'ILU-M-01',
      raw_product_id: 'p1',
      price_per_kg: 800000,
      currency_id: 'tzs',
      lines: [{ harvest_report_id: 'h1', kg: '200.00' }],
    }
    const { result } = renderHook(() => hooks.useCreateBatch(), { wrapper: wrapper() })
    await act(async () => {
      await result.current.mutateAsync(body)
    })
    expect(farmSupplyCreate).toHaveBeenCalledWith('w1', body)
  })
})
