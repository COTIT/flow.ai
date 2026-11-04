import { act, fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useSupplyLinks = vi.fn()
const useSupplyTransfers = vi.fn()
const useDispatchOptions = vi.fn()
const useLotStock = vi.fn()
const dispatch = { mutate: vi.fn(), isPending: false, error: null as unknown, isSuccess: false, reset: vi.fn() }

vi.mock('@/features/ops/supply/useSupply', () => ({
  useSupplyLinks: () => useSupplyLinks(),
  useSupplyTransfers: () => useSupplyTransfers(),
  useDispatchOptions: () => useDispatchOptions(),
  useLotStock: (lotId: string) => useLotStock(lotId),
  useDispatch: () => dispatch,
}))

// The buy card has its own tests.
vi.mock('@/features/ops/supply/BuyHarvestCard', () => ({ BuyHarvestCard: () => null }))

const { DispatchScreen } = await import('@/features/ops/supply/DispatchScreen')

const link = (id: string, status: string, name: string) => ({
  id,
  direction: 'outgoing',
  partner: { id: `w-${id}`, name },
  fields: [],
  status,
  granted_at: '2026-10-30T08:00:00Z',
  accepted_at: null,
  revoked_at: null,
})

beforeEach(() => {
  vi.clearAllMocks()
  dispatch.error = null
  useSupplyLinks.mockReturnValue({
    isLoading: false,
    error: null,
    data: [link('l1', 'live', 'AFM shop'), link('l2', 'pending', 'Other shop')],
  })
  useDispatchOptions.mockReturnValue({
    isLoading: false,
    error: null,
    data: { lots: [{ id: 'lot1', code: 'ILU-H-2026-01' }], warehouses: [{ id: 'wh1', name: 'Iringa store' }] },
  })
  useLotStock.mockImplementation((lotId: string) =>
    lotId
      ? { isLoading: false, error: null, data: [{ warehouse_id: 'wh1', level: '40.00', reserved: '5.00', available: '35.00' }] }
      : { isLoading: false, error: null, data: undefined },
  )
  useSupplyTransfers.mockReturnValue({
    isLoading: false,
    error: null,
    data: [
      {
        id: 't1',
        link_id: 'l1',
        direction: 'outgoing',
        partner: { id: 'w-l1', name: 'AFM shop' },
        from_lot: { code: 'ILU-H-2026-01', public_code: 'ABCDEFGH23' },
        quantity: '40.00',
        status: 'dispatched',
        dispatched_at: '2026-10-31T09:00:00Z',
        to_lot: null,
        received_quantity: null,
        received_at: null,
      },
    ],
  })
})

function fill() {
  fireEvent.change(screen.getByTestId('dispatch-link'), { target: { value: 'l1' } })
  fireEvent.change(screen.getByTestId('dispatch-lot'), { target: { value: 'lot1' } })
  fireEvent.change(screen.getByTestId('dispatch-warehouse'), { target: { value: 'wh1' } })
  fireEvent.change(screen.getByTestId('dispatch-quantity'), { target: { value: '12.50' } })
}

describe('the form', () => {
  test('offers only live links', () => {
    render(<DispatchScreen />)
    const options = Array.from(screen.getByTestId('dispatch-link').querySelectorAll('option[value]:not([value=""])'))
    expect(options.map((o) => o.textContent)).toEqual(['AFM shop'])
  })

  test('send stays off until every field is filled', () => {
    render(<DispatchScreen />)
    expect(screen.getByTestId('dispatch-send')).toBeDisabled()
    fill()
    expect(screen.getByTestId('dispatch-send')).toBeEnabled()
  })

  test('cancel in the dialog sends nothing', () => {
    render(<DispatchScreen />)
    fill()
    fireEvent.click(screen.getByTestId('dispatch-send'))
    expect(screen.getByTestId('confirm-dialog')).toHaveTextContent('12.50')
    expect(screen.getByTestId('confirm-dialog')).toHaveTextContent('ILU-H-2026-01')
    expect(screen.getByTestId('confirm-dialog')).toHaveTextContent('AFM shop')
    fireEvent.click(screen.getByTestId('confirm-dialog-cancel'))
    expect(dispatch.mutate).not.toHaveBeenCalled()
    expect(screen.queryByTestId('confirm-dialog')).toBeNull()
  })

  test('confirm sends exactly one transfer, even on a double tap', () => {
    render(<DispatchScreen />)
    fill()
    fireEvent.click(screen.getByTestId('dispatch-send'))
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    expect(dispatch.mutate).toHaveBeenCalledTimes(1)
    expect(dispatch.mutate.mock.calls[0][0]).toEqual({
      linkId: 'l1',
      lotId: 'lot1',
      warehouseId: 'wh1',
      quantity: '12.50',
      idempotencyKey: expect.stringMatching(/^[0-9a-f-]{36}$/),
    })
  })

  // A resend of the same transfer after a lost reply replays the first answer (one transfer); a
  // changed form or a sent transfer starts a new attempt.
  test('the Idempotency-Key lasts until a transfer is sent, even across edits', () => {
    render(<DispatchScreen />)
    fill()
    const send = () => {
      fireEvent.click(screen.getByTestId('dispatch-send'))
      fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    }
    const settle = (call: number, ok = false) =>
      act(() => {
        if (ok) dispatch.mutate.mock.calls[call][1].onSuccess()
        dispatch.mutate.mock.calls[call][1].onSettled()
      })
    const key = (call: number) => dispatch.mutate.mock.calls[call][0].idempotencyKey

    send()
    settle(0)
    send()
    expect(key(1)).toBe(key(0))

    settle(1)
    fireEvent.change(screen.getByTestId('dispatch-quantity'), { target: { value: '10' } })
    send()
    // The first send may have landed with its reply lost: the same key makes the API refuse a changed body
    // instead of making a second transfer.
    expect(key(2)).toBe(key(1))

    settle(2, true)
    fireEvent.change(screen.getByTestId('dispatch-quantity'), { target: { value: '10' } })
    send()
    expect(key(3)).not.toBe(key(2))
  })

  test("shows the picked lot's stock per warehouse, for reading only", () => {
    render(<DispatchScreen />)
    expect(screen.queryByTestId('dispatch-stock')).toBeNull()
    fireEvent.change(screen.getByTestId('dispatch-lot'), { target: { value: 'lot1' } })

    expect(useLotStock).toHaveBeenLastCalledWith('lot1')
    const row = screen.getByTestId('dispatch-stock-row')
    expect(row).toHaveTextContent('Iringa store')
    expect(row).toHaveTextContent('35.00 available')
    expect(row).toHaveTextContent('40.00 on hand, 5.00 held for orders')
  })

  test('a lot with no stock says so', () => {
    useLotStock.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<DispatchScreen />)
    fireEvent.change(screen.getByTestId('dispatch-lot'), { target: { value: 'lot1' } })
    expect(screen.getByTestId('dispatch-stock')).toHaveTextContent('This lot holds no stock.')
  })

  test("the API's refusal is shown as written", () => {
    dispatch.error = { response: { status: 403, data: { message: 'The supply link is not live.' } } }
    render(<DispatchScreen />)
    expect(screen.getByTestId('dispatch-error')).toHaveTextContent('The supply link is not live.')
  })
})

describe('sent transfers', () => {
  test('lists each with its lot, partner, quantity and state', () => {
    render(<DispatchScreen />)
    const row = screen.getByTestId('supply-transfer-row')
    expect(row).toHaveTextContent('ILU-H-2026-01')
    expect(row).toHaveTextContent('AFM shop')
    expect(row).toHaveTextContent('40.00')
    expect(row).toHaveTextContent('Dispatched')
  })
})
