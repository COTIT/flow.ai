import { fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useSupplyLinks = vi.fn()
const grant = { mutate: vi.fn(), isPending: false, error: null as unknown, isSuccess: false }
const revoke = { mutate: vi.fn(), isPending: false, error: null as unknown }

vi.mock('@/features/ops/supply/useSupply', () => ({
  useSupplyLinks: () => useSupplyLinks(),
  useGrantLink: () => grant,
  useRevokeLink: () => revoke,
}))

const { SupplyLinksScreen } = await import('@/features/ops/supply/SupplyLinksScreen')

const link = (id: string, status: string, fields: string[] = ['village']) => ({
  id,
  direction: 'outgoing',
  partner: { id: 'w-shop', name: 'AFM shop' },
  fields,
  status,
  granted_at: '2026-10-30T08:00:00Z',
  accepted_at: null,
  revoked_at: null,
})

beforeEach(() => {
  vi.clearAllMocks()
  grant.error = null
  revoke.error = null
  useSupplyLinks.mockReturnValue({ isLoading: false, error: null, data: [link('l1', 'live'), link('l2', 'revoked', [])] })
})

describe('the list', () => {
  test('shows each outgoing link with its partner, state and allow-list', () => {
    render(<SupplyLinksScreen />)
    const rows = screen.getAllByTestId('supply-link-row')
    expect(rows).toHaveLength(2)
    expect(rows[0]).toHaveTextContent('AFM shop')
    expect(rows[0]).toHaveTextContent('Live')
    expect(rows[0]).toHaveTextContent('Village')
    expect(rows[1]).toHaveTextContent('Revoked')
  })

  test('a revoked link offers no revoke', () => {
    render(<SupplyLinksScreen />)
    const rows = screen.getAllByTestId('supply-link-row')
    expect(within(rows[0]).getByTestId('supply-link-revoke')).toBeInTheDocument()
    expect(within(rows[1]).queryByTestId('supply-link-revoke')).toBeNull()
  })

  test('a failed read shows the API message', () => {
    useSupplyLinks.mockReturnValue({
      isLoading: false,
      error: { response: { status: 403, data: { message: 'Only workspace admins may do this.' } } },
      data: undefined,
      refetch: vi.fn(),
    })
    render(<SupplyLinksScreen />)
    expect(screen.getByTestId('error-state')).toHaveTextContent('Only workspace admins may do this.')
  })
})

describe('granting', () => {
  test('sends the target workspace and the ticked fields', () => {
    render(<SupplyLinksScreen />)
    fireEvent.change(screen.getByTestId('supply-grant-to'), { target: { value: ' w-shop ' } })
    fireEvent.click(screen.getByTestId('supply-grant-field-village'))
    fireEvent.click(screen.getByTestId('supply-grant-field-harvest_window'))
    fireEvent.click(screen.getByTestId('supply-grant-submit'))

    expect(grant.mutate).toHaveBeenCalledTimes(1)
    expect(grant.mutate.mock.calls[0][0]).toEqual({ toWorkspaceId: 'w-shop', fields: ['village', 'harvest_window'] })
  })

  test('a second tap while the first is in flight sends nothing more', () => {
    render(<SupplyLinksScreen />)
    fireEvent.change(screen.getByTestId('supply-grant-to'), { target: { value: 'w-shop' } })
    fireEvent.click(screen.getByTestId('supply-grant-submit'))
    fireEvent.click(screen.getByTestId('supply-grant-submit'))
    expect(grant.mutate).toHaveBeenCalledTimes(1)
  })

  test("the API's refusal is shown as written", () => {
    grant.error = { response: { status: 409, data: { message: 'A pending or live link already exists.' } } }
    render(<SupplyLinksScreen />)
    expect(screen.getByTestId('supply-grant-error')).toHaveTextContent('A pending or live link already exists.')
  })
})

describe('revoking', () => {
  test('cancel sends nothing, confirm revokes that link', () => {
    render(<SupplyLinksScreen />)
    fireEvent.click(screen.getByTestId('supply-link-revoke'))
    fireEvent.click(screen.getByTestId('confirm-dialog-cancel'))
    expect(revoke.mutate).not.toHaveBeenCalled()

    fireEvent.click(screen.getByTestId('supply-link-revoke'))
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    expect(revoke.mutate).toHaveBeenCalledTimes(1)
    expect(revoke.mutate.mock.calls[0][0]).toBe('l1')
  })
})
