import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

import { PassbookView } from '@/features/passbook/PassbookView'
import { formatPrice } from '@/lib/format'

const tzs = (minor: number) => ({ minor, currency: 'TZS', decimals: 2, currency_id: 'tzs' })
const line = (id: string, kind: string, minor: number, after: number) => ({
  id,
  kind,
  amount: tzs(minor),
  balance_after: tzs(after),
  attribution_id: null,
  payout_id: null,
  correction_id: null,
  reason: null,
  created_at: '2026-11-01T08:00:00Z',
})
const payout = (id: string, minor: number, voided = false) => ({
  id,
  client_ref: 'c',
  person_id: 'p1',
  village_id: 'v1',
  amount: tzs(minor),
  id_type: 'nida',
  paid_by: 'u1',
  paid_at: '2026-11-02T08:00:00Z',
  void: voided ? { reason: 'paid the wrong person', voided_by: 'u2', voided_at: '2026-11-03T08:00:00Z' } : null,
})
const ok = <T,>(data: T) => ({ isLoading: false, error: null, data, refetch: vi.fn() })

describe('the passbook', () => {
  test('balances, lines and payouts as the API sends them', () => {
    render(
      <PassbookView
        passbook={ok({ balances: [tzs(500000)], lines: [line('l2', 'payout', -300000, 500000), line('l1', 'sale_credit', 800000, 800000)] })}
        payouts={ok([payout('po1', 300000), payout('po0', 1000, true)])}
      />,
    )
    expect(screen.getByTestId('passbook-balance-TZS')).toHaveTextContent('5,000.00')
    const rows = screen.getAllByTestId('passbook-line')
    expect(rows).toHaveLength(2)
    expect(rows[0]).toHaveTextContent('Payout')
    expect(rows[0]!.textContent).toContain(formatPrice(tzs(-300000), 'en'))
    expect(rows[1]).toHaveTextContent('Sale credit')
    expect(rows[1]).toHaveTextContent('8,000.00')
    const paid = screen.getAllByTestId('payout-row')
    expect(paid[0]).toHaveTextContent('3,000.00')
    expect(paid[0]).toHaveTextContent('NIDA')
    expect(paid[1]).toHaveTextContent('Voided: paid the wrong person')
  })

  test('no lines yet: an empty state, and no payouts an empty note', () => {
    render(<PassbookView passbook={ok({ balances: [], lines: [] })} payouts={ok([])} />)
    expect(screen.getByTestId('passbook-empty')).toBeVisible()
    expect(screen.getByTestId('payouts-empty')).toBeVisible()
  })

  // A line kind or ID type the bundle does not know yet reads as a word, never as its code.
  test('an unknown line kind or ID type is not shown as its code', () => {
    render(
      <PassbookView
        passbook={ok({ balances: [tzs(100)], lines: [line('l1', 'fee_charge', 100, 100)] })}
        payouts={ok([{ ...payout('po1', 100), id_type: 'pass_port' }])}
      />,
    )
    expect(screen.getByTestId('passbook-line')).not.toHaveTextContent('fee_charge')
    expect(screen.getByTestId('passbook-line')).toHaveTextContent('Other')
    expect(screen.getByTestId('payout-row')).not.toHaveTextContent('pass_port')
    expect(screen.getByTestId('payout-row')).toHaveTextContent('Other')
  })

  // Farmers grow more than honey: the empty passbook speaks of any harvest.
  test('the empty passbook does not assume one crop, in either language', async () => {
    const i18n = (await import('@/i18n')).default
    expect(i18n.t('passbook.emptyDetail', { lng: 'en' })).not.toMatch(/honey/i)
    expect(i18n.t('passbook.emptyDetail', { lng: 'sw' })).not.toMatch(/asali/i)
  })

  test('a failed read offers a retry', () => {
    const passbook = { isLoading: false, error: new Error('down'), data: undefined, refetch: vi.fn() }
    render(<PassbookView passbook={passbook} payouts={ok([])} />)
    fireEvent.click(screen.getByRole('button', { name: /try again/i }))
    expect(passbook.refetch).toHaveBeenCalled()
  })

  test('loading shows a loading state', () => {
    render(<PassbookView passbook={{ isLoading: true, error: null, data: undefined, refetch: vi.fn() }} payouts={ok([])} />)
    expect(screen.getByTestId('passbook-loading')).toBeVisible()
  })

  test('an action per positive balance, when given', () => {
    render(
      <PassbookView
        passbook={ok({ balances: [tzs(500000), { ...tzs(0), currency: 'USD', currency_id: 'usd' }], lines: [line('l1', 'sale_credit', 500000, 500000)] })}
        payouts={ok([])}
        action={(balance) => <button type="button">pay {balance.currency}</button>}
      />,
    )
    expect(screen.getByRole('button', { name: 'pay TZS' })).toBeVisible()
    expect(screen.queryByRole('button', { name: 'pay USD' })).toBeNull()
  })
})
