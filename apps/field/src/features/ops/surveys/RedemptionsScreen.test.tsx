import { act, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

const useRedemptions = vi.fn()
const navigate = vi.fn()
const search = vi.fn()

vi.mock('@/features/ops/surveys/useSurveyAdmin', () => ({
  useRedemptions: (from: string, to: string) => useRedemptions(from, to),
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => search() }),
  useNavigate: () => navigate,
  Link: ({ children, to, ...props }: { children: React.ReactNode; to?: string } & Record<string, unknown>) => (
    <a href={to ?? '#'} {...props}>{children}</a>
  ),
}))

const { RedemptionsScreen } = await import('@/features/ops/surveys/RedemptionsScreen')
const { default: i18n } = await import('@/i18n')

const report = {
  totals: [
    { day: '2026-09-29', redeemed_by: 'u3', redeemed_by_name: 'Asha Officer', vouchers: 2, amount: 10000, currency: 'TZS' },
    { day: '2026-09-28', redeemed_by: 'u4', redeemed_by_name: 'Juma Ops', vouchers: 1, amount: 5000, currency: 'TZS' },
  ],
  log: [
    {
      voucher_id: 'v1',
      redeemed_at: '2026-09-29T06:15:00Z',
      redeemed_by: 'u3',
      redeemed_by_name: 'Asha Officer',
      village_name: 'Ilundo',
      household_label: 'Mwakalinga household',
      survey_title_en: 'Harvest intentions',
      survey_title_sw: 'Nia ya mavuno',
      amount: 5000,
      currency: 'TZS',
      id_type_seen: 'nida',
      audit_required: false,
    },
  ],
}

beforeEach(() => {
  // Only Date is faked: 22:30 UTC on the 28th is already the 29th in Dar.
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date('2026-09-28T22:30:00Z'))
  useRedemptions.mockReset()
  navigate.mockReset()
  search.mockReset()
  search.mockReturnValue({})
  useRedemptions.mockReturnValue({ isLoading: false, error: null, data: report, refetch: vi.fn() })
})

afterEach(async () => {
  vi.useRealTimers()
  await act(async () => {
    await i18n.changeLanguage('en')
  })
})

describe('the date range', () => {
  test('defaults to today in Dar es Salaam back seven days', () => {
    render(<RedemptionsScreen />)

    expect(useRedemptions).toHaveBeenCalledWith('2026-09-22', '2026-09-29')
    expect(screen.getByTestId('redemptions-from')).toHaveValue('2026-09-22')
    expect(screen.getByTestId('redemptions-to')).toHaveValue('2026-09-29')
  })

  // One anchor around both dates: the guided tour invites a change of range,
  // and a spotlight only lets a click through where it cuts a hole.
  test('both dates sit inside one range control', () => {
    render(<RedemptionsScreen />)

    const range = screen.getByTestId('redemptions-range')
    expect(range).toContainElement(screen.getByTestId('redemptions-from'))
    expect(range).toContainElement(screen.getByTestId('redemptions-to'))
  })

  test('is read from the URL', () => {
    search.mockReturnValue({ from: '2026-09-01', to: '2026-09-15' })
    render(<RedemptionsScreen />)
    expect(useRedemptions).toHaveBeenCalledWith('2026-09-01', '2026-09-15')
  })

  // A hand-typed or stale URL degrades to the default rather than reaching the
  // RPC as an invalid date.
  test('ignores a URL value that is not a date', () => {
    search.mockReturnValue({ from: 'last-week', to: '2026-09-15' })
    render(<RedemptionsScreen />)
    expect(useRedemptions).toHaveBeenCalledWith('2026-09-22', '2026-09-15')
  })

  test('changing a date writes it to the URL', () => {
    render(<RedemptionsScreen />)

    fireEvent.change(screen.getByTestId('redemptions-from'), { target: { value: '2026-09-01' } })

    expect(navigate).toHaveBeenCalledWith({
      to: '/ops/surveys/redemptions',
      search: { from: '2026-09-01', to: '2026-09-29' },
      replace: true,
    })
  })

  test('a cleared date falls back to the default', () => {
    search.mockReturnValue({ from: '2026-09-01', to: '2026-09-15' })
    render(<RedemptionsScreen />)

    fireEvent.change(screen.getByTestId('redemptions-to'), { target: { value: '' } })

    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({ search: { from: '2026-09-01', to: undefined } }),
    )
  })
})

describe('the totals', () => {
  test('are per officer per day, as the RPC totalled them', () => {
    render(<RedemptionsScreen />)

    const rows = screen.getAllByTestId('redemption-total-row')
    expect(rows).toHaveLength(2)
    expect(rows[0]).toHaveTextContent('29 Sep 2026')
    expect(rows[0]).toHaveTextContent('Asha Officer')
    expect(rows[0]).toHaveTextContent('2')
    expect(rows[0]).toHaveTextContent('TZS 10,000.00')
  })
})

describe('the log', () => {
  test('lists every handover with its officer, village, household, survey and ID checked', () => {
    render(<RedemptionsScreen />)

    const row = screen.getByTestId('redemption-row')
    expect(row).toHaveTextContent('29 Sep 2026, 09:15')
    expect(row).toHaveTextContent('Asha Officer')
    expect(row).toHaveTextContent('Ilundo')
    expect(row).toHaveTextContent('Mwakalinga household')
    expect(row).toHaveTextContent('Harvest intentions')
    expect(row).toHaveTextContent('TZS 5,000.00')
    expect(row).toHaveTextContent('NIDA card')
  })

  test('the survey title is chosen by language at render', async () => {
    render(<RedemptionsScreen />)
    await act(async () => {
      await i18n.changeLanguage('sw')
    })
    expect(screen.getByTestId('redemption-row')).toHaveTextContent('Nia ya mavuno')
  })
})

describe('states', () => {
  test('loading', () => {
    useRedemptions.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<RedemptionsScreen />)
    expect(screen.getByTestId('redemptions-loading')).toBeInTheDocument()
  })

  test('a failed read is an error, with the range still adjustable', () => {
    useRedemptions.mockReturnValue({ isLoading: false, error: new Error('only ops'), data: undefined, refetch: vi.fn() })
    render(<RedemptionsScreen />)
    expect(screen.getByTestId('error-state')).toHaveTextContent('only ops')
    expect(screen.getByTestId('redemptions-from')).toBeInTheDocument()
  })

  test('nothing handed over is two empty states, not an error', () => {
    useRedemptions.mockReturnValue({ isLoading: false, error: null, data: { totals: [], log: [] } })
    render(<RedemptionsScreen />)
    expect(screen.getAllByTestId('empty-state')).toHaveLength(2)
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('links back to the surveys', () => {
    render(<RedemptionsScreen />)
    expect(within(screen.getByTestId('page-header')).getAllByRole('link')[0]).toHaveAttribute('href', '/ops/surveys')
  })
})
