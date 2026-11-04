import { act, fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

type Mutation = { mutate: ReturnType<typeof vi.fn>; isPending: boolean; error: unknown; isSuccess: boolean; data: unknown; reset: ReturnType<typeof vi.fn> }
let payout: Mutation
vi.mock('@/features/passbook/usePassbook', () => ({ useCreatePayout: () => payout }))
let refs = 0
vi.mock('@/lib/ids', () => ({ newUuid: () => `ref-${++refs}` }))

const { PayOut } = await import('@/features/passbook/PayOut')

const tzs = { minor: 500000, currency: 'TZS', decimals: 2, currency_id: 'tzs' }
const refused = (status: number, body: Record<string, unknown>) => ({ isAxiosError: true, response: { status, data: body } })

beforeEach(() => {
  refs = 0
  payout = { mutate: vi.fn(), isPending: false, error: null, isSuccess: false, data: undefined, reset: vi.fn() }
})

const open = () => {
  render(<PayOut personId="p1" personName="Neema Mwakalinga" balance={tzs} />)
  fireEvent.click(screen.getByTestId('pay-out-TZS'))
}
const fill = () => {
  fireEvent.change(screen.getByTestId('payout-amount'), { target: { value: '3000' } })
  fireEvent.change(screen.getByTestId('payout-id-type'), { target: { value: 'nida' } })
  fireEvent.click(screen.getByTestId('payout-name-confirmed'))
}

describe('pay out', () => {
  test('confirm stays off until amount, ID type and the name check are given', () => {
    open()
    expect(screen.getByTestId('confirm-dialog')).toHaveTextContent('Neema Mwakalinga')
    expect(screen.getByTestId('confirm-dialog-confirm')).toBeDisabled()
    fireEvent.change(screen.getByTestId('payout-amount'), { target: { value: '3000' } })
    fireEvent.change(screen.getByTestId('payout-id-type'), { target: { value: 'nida' } })
    expect(screen.getByTestId('confirm-dialog-confirm')).toBeDisabled()
    fireEvent.click(screen.getByTestId('payout-name-confirmed'))
    expect(screen.getByTestId('confirm-dialog-confirm')).toBeEnabled()
    fireEvent.change(screen.getByTestId('payout-amount'), { target: { value: '30.005' } })
    expect(screen.getByTestId('confirm-dialog-confirm')).toBeDisabled()
  })

  test('posts minor units under a client_ref; the balance check is the API\'s', () => {
    open()
    fill()
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    expect(payout.mutate).toHaveBeenCalledWith(
      { client_ref: 'ref-1', person_id: 'p1', amount: 300000, currency_id: 'tzs', id_type: 'nida', name_confirmed: true },
      expect.anything(),
    )
  })

  test('a retry after a failed send reuses the client_ref; a success starts a new one', () => {
    open()
    fill()
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    const first = payout.mutate.mock.calls[0]![1] as { onError?: () => void; onSettled: () => void; onSuccess: () => void }
    act(() => first.onSettled())
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    expect((payout.mutate.mock.calls[1]![0] as { client_ref: string }).client_ref).toBe('ref-1')

    const second = payout.mutate.mock.calls[1]![1] as { onSuccess: () => void; onSettled: () => void }
    act(() => {
      second.onSuccess()
      second.onSettled()
    })
    fireEvent.click(screen.getByTestId('pay-out-TZS'))
    fill()
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    expect((payout.mutate.mock.calls[2]![0] as { client_ref: string }).client_ref).not.toBe('ref-1')
  })

  test("the API's amount refusal shows under the amount", () => {
    payout.error = refused(422, { message: 'x', errors: { amount: ['The amount must not be greater than the balance of 5000.00.'] } })
    open()
    expect(screen.getByTestId('payout-amount-error')).toHaveTextContent('greater than the balance')
  })

  test("a 403 shows the API's reason, not a generic error", () => {
    payout.error = refused(403, { message: 'you registered this household, so another staff member must pay this farmer' })
    open()
    expect(screen.getByTestId('payout-error')).toHaveTextContent('you registered this household')
  })

  test('a payout made shows a success notice', () => {
    payout.isSuccess = true
    payout.data = { amount: { minor: 300000, currency: 'TZS', decimals: 2, currency_id: 'tzs' } }
    render(<PayOut personId="p1" personName="Neema Mwakalinga" balance={tzs} />)
    expect(screen.getByTestId('payout-done')).toHaveTextContent('3,000.00')
  })
})
