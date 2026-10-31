import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const VILLAGE = '20000000-0000-4000-8000-000000000001'
const search = vi.fn()
const quality = vi.fn()
const rows = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => search() }),
  Link: ({ children, to, ...props }: { children: React.ReactNode; to?: string } & Record<string, unknown>) => (
    <a href={to ?? '#'} {...props}>{children}</a>
  ),
}))
vi.mock('@/app/scope', () => ({
  useScopeNames: () => ({ data: { villages: { [VILLAGE]: 'Ilundo' } }, error: null }),
}))
vi.mock('@/features/tower/useTower', () => ({
  useTowerQuality: () => quality(),
  useTowerQualityRows: () => rows(),
}))

const { TowerQualityScreen } = await import('@/features/tower/TowerQualityScreen')
await import('@/i18n')

beforeEach(() => {
  search.mockReturnValue({ village: VILLAGE, metric: 'persons' })
  quality.mockReturnValue({ data: { persons: 2, persons_verified: 1, farms: 1, farms_with_gps: 0, cycles: 1, cycles_with_estimate: 1 }, isLoading: false, error: null })
  rows.mockReturnValue({ data: [
    { id: 'p1', label: 'Neema Mushi', detail: 'verified', qualifies: true, kind: 'person' },
    { id: 'p2', label: 'Joseph Mushi', detail: 'unverified', qualifies: false, kind: 'person' },
  ], isLoading: false, error: null })
})

describe('Tower data quality trace', () => {
  test('shows view ratio beside each source row and its record link', () => {
    render(<TowerQualityScreen />)
    expect(screen.getByTestId('quality-ratio')).toHaveTextContent('1 / 2')
    expect(screen.getAllByTestId('quality-row')).toHaveLength(2)
    expect(screen.getByRole('link', { name: /Neema Mushi/ })).toHaveAttribute('href', '/officer/people/$personId')
    expect(screen.getByRole('link', { name: /Joseph Mushi/ })).toHaveAttribute('href', '/officer/people/$personId')
  })

  test('zero rows render empty state', () => {
    rows.mockReturnValue({ data: [], isLoading: false, error: null })
    render(<TowerQualityScreen />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
  })
})
