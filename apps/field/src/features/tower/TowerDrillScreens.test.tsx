import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const production = vi.fn()
const productionRows = vi.fn()
const energy = vi.fn()
const energyRows = vi.fn()
const market = vi.fn()
const scope = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => ({ village: 'v1' }) }),
  Link: ({ children, to, ...props }: { children: React.ReactNode; to?: string } & Record<string, unknown>) => (
    <a href={to ?? '#'} {...props}>{children}</a>
  ),
}))
vi.mock('@/app/scope', () => ({ useScopeNames: () => scope() }))
vi.mock('@/features/tower/useTower', () => ({
  useTowerProduction: () => production(),
  useTowerProductionRows: () => productionRows(),
  useTowerEnergy: () => energy(),
  useTowerEnergyRows: () => energyRows(),
  useTowerMarket: () => market(),
}))

const { TowerProductionScreen, TowerEnergyScreen, TowerMarketScreen } = await import('@/features/tower/TowerDrillScreens')
await import('@/i18n')

beforeEach(() => {
  scope.mockReset()
  production.mockReset(); productionRows.mockReset(); energy.mockReset(); energyRows.mockReset(); market.mockReset()
  scope.mockReturnValue({ data: { villages: { v1: 'Ilundo' } } })
  production.mockReturnValue({ isLoading: false, error: null, data: [{ crop_id: 'c1', crop_name: 'Maize', window_month: '2026-09-01', expected_kg: 12000, actual_kg: null }] })
  productionRows.mockReturnValue({ isLoading: false, error: null, data: [{ id: 'cycle1', crop_name: 'Maize', person_id: 'p1', farmer: 'Neema', plot_label: 'Kipande', harvest_start: '2026-09-01', harvest_end: '2026-09-30', area_ha: 1.2, expected_kg: 12000, actual_kg: null }] })
  energy.mockReturnValue({ isLoading: false, error: null, data: { prospective_kw_raw: 12, approved_kw_raw: 15, simultaneity_factor: 0.6, prospective_peak_kw: 7.2, approved_peak_kw: 9 } })
  energyRows.mockReturnValue({ isLoading: false, error: null, data: [{ id: 'r1', applicant: 'Baraka', equipment_name: 'Dryer', status: 'under_review', est_power_kw: 12 }] })
  market.mockReturnValue({ isLoading: false, error: null, data: { matches: [{ buyer_demand_id: 'd1', village_id: 'v1', buyer_name: 'Buyer', crop_name: 'Maize', window_start: '2026-09-01', window_end: '2026-09-30', demand_kg: 9000, available_kg: 5600, coverage_pct: 62.2, opportunity_id: 'o1', opportunity_status: 'proposed' }] } })
})

describe('Tower drill-down screens', () => {
  test('production uses a responsive table surface and logical breadcrumb back link', () => {
    render(<TowerProductionScreen />)
    expect(screen.getByTestId('production-table')).toBeInTheDocument()
    expect(screen.getByTestId('production-row')).toHaveTextContent('Maize')
    expect(screen.getByRole('link', { name: /Back to the Tower/ })).toHaveAttribute('href', '/ops/tower')
    expect(screen.getByTestId('production-table').parentElement).toHaveClass('overflow-x-auto')
    expect(screen.getByTestId('production-table').closest('section')?.parentElement).toHaveClass('w-full')
  })

  test('energy keeps prospective and approved figures in separate groups', () => {
    render(<TowerEnergyScreen />)
    expect(screen.getByTestId('energy-group-prospective')).toBeInTheDocument()
    expect(screen.getByTestId('energy-group-approved')).toBeInTheDocument()
    expect(screen.queryByTestId('energy-excluded')).not.toBeInTheDocument()
    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toBeInTheDocument()
  })

  test('market renders matching supply and honest empty/error states', () => {
    const { rerender } = render(<TowerMarketScreen />)
    expect(screen.getByTestId('market-table')).toBeInTheDocument()
    expect(screen.getByTestId('market-row')).toHaveTextContent('Buyer')

    market.mockReturnValue({ isLoading: false, error: null, data: { matches: [] } })
    rerender(<TowerMarketScreen />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()

    market.mockReturnValue({ isLoading: false, error: new Error('Market failed'), data: null })
    rerender(<TowerMarketScreen />)
    expect(screen.getByRole('alert')).toHaveTextContent('Market failed')
  })
})
