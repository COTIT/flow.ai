import { render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'

// A real uuid: validateVillageSearch discards anything that is not one, which
// is how a nonsense village in the URL fails to break the screen.
const VILLAGE = '20000000-0000-4000-8000-000000000001'

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, ...rest }: { children: React.ReactNode } & Record<string, unknown>) => (
    <a href="#x" {...rest}>
      {children}
    </a>
  ),
  useNavigate: () => () => {},
  getRouteApi: () => ({ useSearch: () => ({ village: VILLAGE }) }),
}))

const flags = vi.hoisted(() => ({ resources: true }))
vi.mock('@/lib/features', () => ({
  get resourcesEnabled() {
    return flags.resources
  },
}))

vi.mock('@/app/scope', () => ({
  useScopeNames: () => ({
    data: { villages: { [VILLAGE]: 'Ilundo' }, projects: { p1: 'Ruaha Ilundo Programme' } },
    error: null,
  }),
}))

const ENERGY = {
  capacity_kw: 500,
  capacity_basis: 'planned',
  simultaneity_factor: 0.6,
  prospective_peak_kw: 7.2,
  approved_peak_kw: 10.8,
  headroom_kw: 489.2,
  approved_kwh_per_week: 324,
}

vi.mock('@/features/tower/useTower', () => ({
  useTowerProduction: () => ({
    data: [
      {
        crop_id: 'maize',
        crop_name: 'Mahindi',
        window_month: '2026-09-01',
        expected_kg: 12000,
        actual_kg: null,
        cycle_area_ha: 4.9,
      },
    ],
    isLoading: false,
    error: null,
    refetch: () => {},
  }),
  useTowerPipeline: () => ({
    data: [
      { status: 'approved', request_count: 2, indicative_value: 38600000, currency: 'TZS', indicative_rent_per_day: null },
      { status: 'submitted', request_count: 1, indicative_value: null, currency: 'TZS', indicative_rent_per_day: 150000 },
    ],
    isLoading: false,
    error: null,
  }),
  useTowerEnergy: () => ({ data: ENERGY, isLoading: false, error: null }),
  useTowerMarket: () => ({
    data: {
      matches: [
        {
          buyer_demand_id: 'd1',
          buyer_name: 'Iringa Grain Traders — DEMO',
          crop_name: 'Mahindi',
          demand_kg: 9000,
          available_kg: 5600,
          committed_kg: 6400,
          coverage_pct: 62.2,
        },
      ],
    },
    isLoading: false,
    error: null,
  }),
  useTowerQuality: () => ({
    data: {
      persons: 6,
      persons_verified: 3,
      farms: 4,
      farms_with_gps: 3,
      cycles: 7,
      cycles_with_estimate: 5,
    },
    isLoading: false,
    error: null,
  }),
}))

const { TowerScreen } = await import('@/features/tower/TowerScreen')
await import('@/i18n')

/**
 * The Tower is the demo's destination, and it is the screen where a layout
 * decision can quietly become a claim about the programme. These are the
 * claims, asserted as layout.
 */
describe('the Control Tower keeps its figures apart', () => {
  test('capacity carries its basis inside its own cell, where sorting cannot separate them', () => {
    render(<TowerScreen />)
    const capacity = screen.getByTestId('tower-capacity')

    expect(capacity).toHaveTextContent('500.000 kW')
    expect(capacity, 'the basis travels with the figure').toHaveTextContent(/planned/i)
  })

  test('capacity is never described as measured', () => {
    render(<TowerScreen />)
    expect(screen.getByTestId('tile-energy')).not.toHaveTextContent(/measured consumption is/i)
    expect(screen.getByTestId('tile-energy')).toHaveTextContent(/planned/i)
  })

  /**
   * Prospective and approved are separate figures and are never summed. The
   * layout has to make that impossible to misread, not merely say it: two
   * framed cells with a rule between them, never one stacked bar.
   */
  test('prospective and approved sit in two cells with a rule between them', () => {
    render(<TowerScreen />)

    const prospective = screen.getByTestId('tower-prospective-peak')
    const approved = screen.getByTestId('tower-approved-peak')

    const prospectiveCell = prospective.closest('[data-peak-cell]')
    const approvedCell = approved.closest('[data-peak-cell]')

    expect(prospectiveCell, 'prospective has a cell of its own').not.toBeNull()
    expect(approvedCell, 'approved has a cell of its own').not.toBeNull()
    expect(prospectiveCell).not.toBe(approvedCell)
    expect(prospectiveCell!.parentElement).toBe(approvedCell!.parentElement)
  })

  test('and the sentence that says so is body text, not fine print', () => {
    render(<TowerScreen />)
    const sentence = screen.getByText(/never added together/i)

    expect(sentence.getAttribute('style') ?? '').toMatch(/font-size:\s*13px/)
  })

  test('the headroom bar carries approved peak only, and says so', () => {
    render(<TowerScreen />)
    const tile = screen.getByTestId('tile-energy')

    expect(screen.getByTestId('tower-headroom')).toHaveTextContent('489.200 kW')
    expect(tile).toHaveTextContent(/prospective requests are not on this bar/i)
  })
})

describe('the Control Tower keeps planted area honest', () => {
  test('planted area is named as area across cycles, and disclaims land area', () => {
    render(<TowerScreen />)
    const tile = screen.getByTestId('tile-production')

    expect(tile).toHaveTextContent('4.9000 ha')
    expect(tile).toHaveTextContent(/planted area across cycles/i)
    expect(tile).toHaveTextContent(/it is not land area/i)
  })

  test('the equipment pipeline says its value is indicative', () => {
    render(<TowerScreen />)
    expect(screen.getByTestId('tile-pue')).toHaveTextContent(/indicative/i)
  })

  // A rent is not a purchase price: it is shown apart, per day, never added in.
  test('requests to rent show their rent per day, apart from purchase value', () => {
    render(<TowerScreen />)
    expect(screen.getByTestId('tile-pue-rent')).toHaveTextContent('TZS 150,000.00 rent per day')
    expect(screen.getAllByTestId('tile-pue-rent')).toHaveLength(1)
  })
})

describe('the data quality tile', () => {
  test('links every quality measure to its contributing records', () => {
    render(<TowerScreen />)
    const tile = screen.getByTestId('tile-quality')
    expect(tile.querySelector('[data-testid="tile-drill"]')).toBeInTheDocument()
    expect(tile.querySelectorAll('a')).toHaveLength(4)
  })

  test('states every ratio as n of total, never as a bare percentage', () => {
    render(<TowerScreen />)
    const tile = screen.getByTestId('tile-quality')

    expect(tile).toHaveTextContent('3 / 6')
    expect(tile).toHaveTextContent('3 / 4')
    expect(tile).toHaveTextContent('5 / 7')
  })
})

describe('the tiles are deliberately not uniform', () => {
  // A wrapping flex row, not a grid: the tiles carry different amounts and are
  // sized for what they carry. They stack on a phone without a media query.
  test('each tile declares its own width, and none of them can overflow', () => {
    render(<TowerScreen />)

    for (const [id, basis] of [
      ['production', '430px'],
      ['energy', '350px'],
      ['market', '340px'],
      ['pue', '280px'],
      ['quality', '230px'],
    ] as const) {
      const style = screen.getByTestId(`tile-${id}`).getAttribute('style') ?? ''
      expect(style, id).toContain(basis)
      expect(style, id).toMatch(/min-width:\s*0/)
    }
  })
})

// 25 Oct 2026: the equipment pipeline is held back from the first release.
describe('with the resource flag off', () => {
  afterEach(() => {
    flags.resources = true
  })

  test('the equipment pipeline tile is not shown, and energy still is', () => {
    flags.resources = false
    render(<TowerScreen />)

    expect(screen.queryByTestId('tile-pue')).not.toBeInTheDocument()
    expect(screen.getByTestId('tile-energy')).toBeInTheDocument()
  })
})
