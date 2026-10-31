import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useMyFarm = vi.fn()
vi.mock('@/features/farmer/useMyFarm', () => ({ useMyFarm: () => useMyFarm() }))

const { MyFarmScreen } = await import('@/features/farmer/MyFarmScreen')
await import('@/i18n')

beforeEach(() => useMyFarm.mockReset())

const farm = (over: Record<string, unknown> = {}) => ({
  id: 'f1',
  label: 'Shamba la Neema',
  latitude: null,
  longitude: null,
  source: 'field_verified',
  verification: 'verified',
  confidence: 'high',
  captured_at: '2026-09-09T21:30:00Z',
  plots: [],
  ...over,
})

describe('MyFarmScreen states', () => {
  test('loading shows a loading state, not an empty message', () => {
    useMyFarm.mockReturnValue({ isLoading: true, error: null, farms: [] })
    render(<MyFarmScreen />)

    expect(screen.getByTestId('my-farm-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
  })

  test('an error is an error, offering retry', () => {
    useMyFarm.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      farms: [],
      refetch: vi.fn(),
    })
    render(<MyFarmScreen />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
    expect(screen.getByText('could not reach the database')).toBeInTheDocument()
    expect(screen.getByTestId('error-retry')).toBeInTheDocument()
  })

  // Zero farms is a legitimate answer for a farmer nobody has registered yet.
  // It gets a real message rather than an empty list, and never an error.
  test('no farm yet is a real message, not a blank list', () => {
    useMyFarm.mockReturnValue({ isLoading: false, error: null, farms: [] })
    render(<MyFarmScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.getByText('No farm recorded yet')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('a farm with no plots says so rather than rendering nothing', () => {
    useMyFarm.mockReturnValue({ isLoading: false, error: null, farms: [farm()] })
    render(<MyFarmScreen />)

    expect(screen.getByTestId('farm-card')).toBeInTheDocument()
    expect(screen.getByText('No plots recorded on this farm yet')).toBeInTheDocument()
  })

  test('a plot with no cycles says so', () => {
    useMyFarm.mockReturnValue({
      isLoading: false,
      error: null,
      farms: [
        farm({
          plots: [
            {
              id: 'p1',
              label: 'Kipande cha juu',
              area_ha: 1.8,
              source: 'field_verified',
              verification: 'verified',
              confidence: 'high',
              captured_at: '2026-09-09T21:30:00Z',
              cycles: [],
            },
          ],
        }),
      ],
    })
    render(<MyFarmScreen />)

    expect(screen.getByText('1.8000 ha')).toBeInTheDocument()
    expect(screen.getByText('No crops recorded on this plot yet')).toBeInTheDocument()
  })

  // A cycle can exist before anyone has estimated a harvest. That absence must
  // not render as 0 kg.
  test('a cycle with no expected figure omits it rather than showing zero', () => {
    useMyFarm.mockReturnValue({
      isLoading: false,
      error: null,
      farms: [
        farm({
          plots: [
            {
              id: 'p1',
              label: 'Kipande cha juu',
              area_ha: 1.8,
              source: 'field_verified',
              verification: 'verified',
              confidence: 'high',
              captured_at: '2026-09-09T21:30:00Z',
              cycles: [
                {
                  id: 'c1',
                  crop_name: 'Mahindi',
                  season_label: null,
                  area_ha: 1.6,
                  tree_count: null,
                  unit_count: null,
                  harvest_start: '2026-09-01',
                  harvest_end: '2026-09-30',
                  source: 'field_verified',
                  verification: 'verified',
                  confidence: 'high',
                  captured_at: '2026-09-09T21:30:00Z',
                  expected: null,
                },
              ],
            },
          ],
        }),
      ],
    })
    render(<MyFarmScreen />)

    expect(screen.getByText('Mahindi')).toBeInTheDocument()
    expect(screen.queryByText('0.00 kg')).not.toBeInTheDocument()
    expect(screen.queryByText('Expected harvest:')).not.toBeInTheDocument()
  })

  test('the screen states plainly that it is read-only', () => {
    useMyFarm.mockReturnValue({ isLoading: false, error: null, farms: [farm()] })
    render(<MyFarmScreen />)

    expect(screen.getByTestId('my-farm')).toHaveTextContent(/ask your field officer/i)
  })
})
