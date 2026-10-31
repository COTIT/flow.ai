import { render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useVoucher = vi.fn()
const useVoucherCode = vi.fn()
const useVoucherTimeline = vi.fn()

vi.mock('@/features/farmer/useVouchers', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/features/farmer/useVouchers')>()
  return {
    ...actual,
    useVoucher: (id: string) => useVoucher(id),
    useVoucherCode: (id: string) => useVoucherCode(id),
    useVoucherTimeline: (id: string) => useVoucherTimeline(id),
  }
})

const { VoucherCard } = await import('@/features/farmer/VoucherCard')
await import('@/i18n')

const VOUCHER = '51000000-0000-4000-8000-000000000001'

// Far enough ahead and behind that the test does not depend on today's date.
const FUTURE = '2099-01-15T09:00:00Z'
const PAST = '2020-01-15T09:00:00Z'

const voucher = (over: Record<string, unknown> = {}) => ({
  id: VOUCHER,
  response_id: 'r1',
  survey_id: 's1',
  household_id: 'h1',
  village_id: 'v1',
  amount: 5000,
  currency: 'TZS',
  status: 'issued',
  issued_at: '2026-09-20T08:00:00Z',
  expires_at: FUTURE,
  redeemed_by: null,
  redeemed_at: null,
  id_type_seen: null,
  voided_by: null,
  voided_at: null,
  void_reason: null,
  ...over,
})

const issuedEvents = [
  { occurred_at: '2026-09-20T08:00:00Z', kind: 'answered', actor_name: 'Amina Farmer', actor_role: 'farmer', detail: {} },
  { occurred_at: '2026-09-20T08:00:01Z', kind: 'issued', actor_name: 'Amina Farmer', actor_role: 'farmer', detail: {} },
]

function given({
  row = voucher(),
  code = 'K7QXM2PA9D' as string | null,
  events = issuedEvents as unknown[],
} = {}) {
  useVoucher.mockReturnValue({ isLoading: false, error: null, data: row, refetch: vi.fn() })
  useVoucherCode.mockReturnValue({ isLoading: false, error: null, data: code })
  useVoucherTimeline.mockReturnValue({ isLoading: false, error: null, data: events, refetch: vi.fn() })
}

beforeEach(() => {
  useVoucher.mockReset()
  useVoucherCode.mockReset()
  useVoucherTimeline.mockReset()
})

describe('VoucherCard states', () => {
  test('loading is a loading state, not an empty one', () => {
    given()
    useVoucher.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByTestId('voucher-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
  })

  test('a failed read is an error, offering retry', () => {
    given()
    useVoucher.mockReturnValue({ isLoading: false, error: new Error('voucher read failed'), data: undefined, refetch: vi.fn() })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByTestId('error-state')).toHaveTextContent('voucher read failed')
    expect(screen.getByTestId('error-retry')).toBeInTheDocument()
  })

  // Zero rows is an answer: RLS does not show this voucher.
  test('a voucher that is not visible is an empty state, not an error', () => {
    given({ row: null as never })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('asks for the voucher it was given', () => {
    given()
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(useVoucher).toHaveBeenCalledWith(VOUCHER)
    expect(useVoucherCode).toHaveBeenCalledWith(VOUCHER)
    expect(useVoucherTimeline).toHaveBeenCalledWith(VOUCHER)
  })
})

describe('an uncollected voucher', () => {
  test('shows the code in two halves, large', () => {
    given()
    render(<VoucherCard voucherId={VOUCHER} />)

    const code = screen.getByTestId('voucher-code')
    expect(code).toHaveTextContent('K7QXM-2PA9D')
    expect(code).toHaveClass('type-figure')
  })

  test('shows a QR code a scanner can read, labelled with the code', () => {
    given()
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByRole('img', { name: /K7QXM-2PA9D/ })).toBeInTheDocument()
  })

  test('states the incentive and where it is paid', () => {
    given()
    render(<VoucherCard voucherId={VOUCHER} />)

    const amount = screen.getByTestId('voucher-amount')
    expect(amount).toHaveTextContent('TZS 5,000.00')
    expect(screen.getByTestId('voucher-card')).toHaveTextContent(/paid in cash at the Ruaha office/i)
  })

  test('says until when it is valid, and how to use it', () => {
    given()
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByTestId('voucher-validity')).toHaveTextContent(/valid until 15 Jan 2099/i)
    expect(screen.getByTestId('voucher-card')).toHaveTextContent(/show this code at the Ruaha office/i)
  })

  test('carries a status pill', () => {
    given()
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(within(screen.getByTestId('voucher-card')).getByTestId('status-pill')).toHaveTextContent('Not yet collected')
  })

  test('shows the audit trail under its own heading', () => {
    given()
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByRole('heading', { name: 'Audit trail' })).toBeInTheDocument()
    expect(screen.getByTestId('audit-event-issued')).toBeInTheDocument()
    expect(screen.getByTestId('audit-event-answered')).toBeInTheDocument()
  })

  test('the code is still loading: no QR yet, and no invented code', () => {
    given()
    useVoucherCode.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.queryByRole('img')).not.toBeInTheDocument()
    expect(screen.getByTestId('voucher-code-loading')).toBeInTheDocument()
  })

  // NULL is the database saying this code is not yours to read.
  test('a code the database withholds is not drawn', () => {
    given({ code: null })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.queryByRole('img')).not.toBeInTheDocument()
    expect(screen.queryByTestId('voucher-code')).not.toBeInTheDocument()
  })
})

describe('a collected voucher', () => {
  const events = [
    ...issuedEvents,
    { occurred_at: '2026-09-25T10:30:00Z', kind: 'redeemed', actor_name: 'Juma Officer', actor_role: 'field_officer', detail: { id_type_seen: 'nida' } },
  ]

  test('says when and by whom it was handed over', () => {
    given({ row: voucher({ status: 'redeemed', redeemed_at: '2026-09-25T10:30:00Z', redeemed_by: 'u7' }), events })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByTestId('voucher-validity')).toHaveTextContent(
      'Collected 25 Sep 2026, 13:30 · handed over by Juma Officer',
    )
  })

  // A used voucher must not look usable.
  test('shows no QR code and no instruction to bring it', () => {
    given({ row: voucher({ status: 'redeemed', redeemed_at: '2026-09-25T10:30:00Z' }), events })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.queryByRole('img')).not.toBeInTheDocument()
    expect(screen.getByTestId('voucher-card')).not.toHaveTextContent(/show this code/i)
    expect(screen.getByTestId('status-pill')).toHaveTextContent('Collected')
  })
})

describe('a cancelled voucher', () => {
  test('gives the reason it was cancelled, and no QR code', () => {
    given({ row: voucher({ status: 'void', voided_at: '2026-09-22T10:00:00Z', void_reason: 'duplicate household' }) })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByTestId('voucher-validity')).toHaveTextContent('Cancelled: duplicate household')
    expect(screen.queryByRole('img')).not.toBeInTheDocument()
    expect(screen.getByTestId('status-pill')).toHaveTextContent('Cancelled')
  })
})

describe('an expired voucher', () => {
  // `expired` is not stored: the row still says `issued`.
  test('is derived from its date, with no QR code', () => {
    given({ row: voucher({ status: 'issued', expires_at: PAST }) })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByTestId('status-pill')).toHaveTextContent('Expired')
    expect(screen.getByTestId('voucher-validity')).toHaveTextContent('This voucher expired on 15 Jan 2020, 12:00.')
    expect(screen.queryByRole('img')).not.toBeInTheDocument()
  })
})

describe('the timeline', () => {
  test('still loading does not hold up the voucher', () => {
    given()
    useVoucherTimeline.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByTestId('voucher-code')).toBeInTheDocument()
    expect(screen.getByTestId('voucher-timeline-loading')).toBeInTheDocument()
  })

  test('a failed trail is an error in its own section, not over the voucher', () => {
    given()
    useVoucherTimeline.mockReturnValue({ isLoading: false, error: new Error('timeline failed'), data: undefined, refetch: vi.fn() })
    render(<VoucherCard voucherId={VOUCHER} />)

    expect(screen.getByTestId('voucher-code')).toBeInTheDocument()
    expect(screen.getByTestId('error-state')).toHaveTextContent('timeline failed')
  })
})

/**
 * An incentive is a fixed cash amount per household per survey, paid at the
 * Ruaha office. It is not earnings, not a balance and not a payment — and it
 * is not "indicative": the amount is fixed.
 */
describe('the language of the incentive', () => {
  test.each(['issued', 'redeemed', 'void'])('%s never calls it earnings, a wallet, a balance or a payment', (status) => {
    given({ row: voucher({ status, redeemed_at: '2026-09-25T10:30:00Z', void_reason: 'duplicate household' }) })
    render(<VoucherCard voucherId={VOUCHER} />)

    const card = screen.getByTestId('voucher-card')
    expect(card).not.toHaveTextContent(/earning|wallet|balance|payment|indicative/i)
  })
})
