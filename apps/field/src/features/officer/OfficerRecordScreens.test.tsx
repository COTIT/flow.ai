import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useFarmDetail = vi.fn()
const useCycleDetail = vi.fn()
const queueMutate = vi.fn()
const queueState = { isPending: false, variables: undefined as { table: string; id: string } | undefined }
const search: { plot?: string; harvest?: string } = {}
const crops = [
  { id: 'crop-area', name: 'Maize', measured_by: 'area' as const },
  { id: 'crop-tree', name: 'Avocado', measured_by: 'tree_count' as const },
  { id: 'crop-unit', name: 'Eggs', measured_by: 'unit_count' as const },
]

vi.mock('@/features/officer/useOfficerRecords', () => ({
  useFarmDetail: () => useFarmDetail(),
  useCycleDetail: () => useCycleDetail(),
}))
vi.mock('@/features/officer/useVerifyQueue', () => ({
  useVerifyFromQueue: () => ({ ...queueState, mutate: queueMutate, mutateAsync: queueMutate }),
}))
vi.mock('@/features/officer/useCrops', () => ({
  useCrops: () => ({ crops, isLoading: false }),
}))
vi.mock('@/features/officer/RecordEditAction', () => ({
  RecordEditAction: ({ table, id, version }: { table: string; id: string; version?: string | null }) => (
    <button type="button" data-testid={`edit-${table}-${id}`} data-version={version ?? ''}>Edit</button>
  ),
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({
    useParams: () => ({ farmId: 'f1', cycleId: 'c1' }),
    useSearch: () => search,
  }),
}))

const { OfficerFarmScreen, OfficerCycleScreen } = await import(
  '@/features/officer/OfficerRecordScreens'
)
await import('@/i18n')

beforeEach(() => {
  useFarmDetail.mockReset()
  useCycleDetail.mockReset()
  queueMutate.mockReset()
  queueState.isPending = false
  queueState.variables = undefined
  delete search.plot
  delete search.harvest
})

const provenance = {
  source: 'field_verified',
  verification: 'verified',
  confidence: 'high',
  captured_at: '2026-09-09T21:30:00Z',
}

const farm = (over: Record<string, unknown> = {}) => ({
  id: 'f1',
  label: 'Shamba la Neema',
  village_id: 'v1',
  latitude: -8.1301,
  longitude: 35.1892,
  ...provenance,
  plots: [{ id: 'pl1', label: 'Kipande cha juu', area_ha: 1.8, latitude: null, longitude: null, ...provenance }],
  ...over,
})

const cycle = (over: Record<string, unknown> = {}) => ({
  id: 'c1',
  village_id: 'v1',
  crop_name: 'Maize',
  crop_id: 'crop-area',
  season_label: 'Msimu 2026 A',
  status: 'growing',
  area_ha: 1.6,
  tree_count: null,
  unit_count: null,
  planted_on: '2026-03-05',
  harvest_start: '2026-09-01',
  harvest_end: '2026-09-30',
  plot_label: 'Kipande cha juu',
  ...provenance,
  harvests: [
    { id: 'h1', kind: 'expected', quantity_kg: 4100, reported_for: '2026-09-15', is_current: true, ...provenance },
    { id: 'h2', kind: 'expected', quantity_kg: 3200, reported_for: '2026-06-01', is_current: false, ...provenance },
  ],
  ...over,
})

/** Spec 5.5 — farm detail with correction controls. */
describe('OfficerFarmScreen', () => {
  test('loading shows a loading state', () => {
    useFarmDetail.mockReturnValue({ isLoading: true, error: null, data: null })
    render(<OfficerFarmScreen />)
    expect(screen.getByTestId('farm-detail-loading')).toBeInTheDocument()
  })

  test('an error is an error', () => {
    useFarmDetail.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      data: null,
      refetch: vi.fn(),
    })
    render(<OfficerFarmScreen />)
    expect(screen.getByTestId('error-state')).toBeInTheDocument()
  })

  // Zero rows means the farm does not exist, or RLS puts it outside this
  // officer's villages. That is an answer, never an error.
  test('a farm that is not visible is an empty state, not an error', () => {
    useFarmDetail.mockReturnValue({ isLoading: false, error: null, data: null })
    render(<OfficerFarmScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('shows the farm, its plots with area, and provenance on each', () => {
    useFarmDetail.mockReturnValue({ isLoading: false, error: null, data: farm() })
    render(<OfficerFarmScreen />)

    expect(screen.getByTestId('farm-detail')).toHaveTextContent('Shamba la Neema')
    expect(screen.getByTestId('farm-plot')).toHaveTextContent('Kipande cha juu')
    expect(screen.getByTestId('farm-plot')).toHaveTextContent('1.8000 ha')
    // Farm-level plus plot-level.
    expect(screen.getAllByTestId('provenance-badge')).toHaveLength(2)
  })

  test('a farm with no plots says so rather than rendering nothing', () => {
    useFarmDetail.mockReturnValue({ isLoading: false, error: null, data: farm({ plots: [] }) })
    render(<OfficerFarmScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByTestId('farm-plot')).not.toBeInTheDocument()
  })

  test('focuses a requested plot', () => {
    search.plot = 'pl1'
    useFarmDetail.mockReturnValue({ isLoading: false, error: null, data: farm() })
    render(<OfficerFarmScreen />)
    expect(screen.getByTestId('farm-plot')).toHaveAttribute('data-focused', 'true')
  })

  test('an uncaptured GPS point says so rather than showing a null island', () => {
    useFarmDetail.mockReturnValue({
      isLoading: false,
      error: null,
      data: farm({ latitude: null, longitude: null }),
    })
    render(<OfficerFarmScreen />)

    expect(screen.getByTestId('farm-detail')).toHaveTextContent('Not captured')
    expect(screen.getByTestId('farm-detail')).not.toHaveTextContent('0, 0')
  })

  test('handles nullable farm metadata and pending verify state', () => {
    queueState.isPending = true
    queueState.variables = { table: 'farm', id: 'f1' }
    useFarmDetail.mockReturnValue({ isLoading: false, error: null, data: farm({ confidence: null, latitude: null, longitude: null, plots: [{ ...farm().plots[0], confidence: null, area_ha: null }] }) })
    render(<OfficerFarmScreen />)
    expect(screen.getByTestId('farm-detail')).toHaveTextContent('Not captured')
  })

  test('retries farm read errors', async () => {
    const retry = vi.fn()
    useFarmDetail.mockReturnValue({ isLoading: false, error: new Error('farm failed'), data: null, refetch: retry })
    render(<OfficerFarmScreen />)
    await userEvent.setup().click(screen.getByTestId('error-retry'))
    expect(retry).toHaveBeenCalled()
  })

  test('offers section edit controls', () => {
    useFarmDetail.mockReturnValue({ isLoading: false, error: null, data: farm() })
    render(<OfficerFarmScreen />)

    expect(screen.getByTestId('edit-farm-f1')).toBeInTheDocument()
    expect(screen.getByTestId('edit-plot-pl1')).toBeInTheDocument()
  })

  test('verification controls call the queue mutation only after confirmation', async () => {
    const user = userEvent.setup()
    useFarmDetail.mockReturnValue({ isLoading: false, error: null, data: farm({ verification: 'unverified', plots: [{ ...farm().plots[0], verification: 'unverified' }] }) })
    render(<OfficerFarmScreen />)
    await user.click(screen.getByTestId('verify-farm-f1'))
    await user.click(screen.getByTestId('confirm-dialog-confirm'))
    await user.click(screen.getByTestId('verify-plot-pl1'))
    await user.click(screen.getByTestId('confirm-dialog-confirm'))
    await waitFor(() => expect(queueMutate).toHaveBeenCalledTimes(2))
  })
})

/** Spec 5.6 — cycle detail with correction controls. */
describe('OfficerCycleScreen', () => {
  test('loading shows a loading state', () => {
    useCycleDetail.mockReturnValue({ isLoading: true, error: null, cycle: null })
    render(<OfficerCycleScreen />)
    expect(screen.getByTestId('cycle-detail-loading')).toBeInTheDocument()
  })
  test('a cycle that is not visible is an empty state', () => {
    useCycleDetail.mockReturnValue({ isLoading: false, error: null, cycle: null })
    render(<OfficerCycleScreen />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
  })

  test('an error is an error', () => {
    useCycleDetail.mockReturnValue({ isLoading: false, error: new Error('cycle failed'), cycle: null, refetch: vi.fn() })
    render(<OfficerCycleScreen />)
    expect(screen.getByTestId('error-state')).toHaveTextContent('cycle failed')
  })

  test('shows the crop, plot, window and measure', () => {
    useCycleDetail.mockReturnValue({ isLoading: false, error: null, cycle: cycle() })
    render(<OfficerCycleScreen />)

    const detail = screen.getByTestId('cycle-detail')
    expect(detail).toHaveTextContent('Maize')
    expect(detail).toHaveTextContent('Kipande cha juu')
    expect(detail).toHaveTextContent('1.6000 ha')
    expect(detail).toHaveTextContent('Growing')
  })

  // A harvest figure is a SERIES, not a value: the superseded row stays
  // visible and labelled so a revision has an audit trail (business-rules §6).
  test('shows the current figure and the one it replaced, labelled', () => {
    useCycleDetail.mockReturnValue({ isLoading: false, error: null, cycle: cycle() })
    render(<OfficerCycleScreen />)

    const rows = screen.getAllByTestId('cycle-harvest')
    expect(rows).toHaveLength(2)
    expect(rows[0]).toHaveTextContent('4,100.00 kg')
    expect(rows[0]).not.toHaveTextContent(/superseded/i)
    expect(rows[1]).toHaveTextContent('3,200.00 kg')
    expect(rows[1]).toHaveTextContent(/superseded/i)
  })

  test('a cycle with no harvest figure says so', () => {
    useCycleDetail.mockReturnValue({ isLoading: false, error: null, cycle: cycle({ harvests: [] }) })
    render(<OfficerCycleScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
  })

  // The measure branches on crop.measured_by: exactly one of area, trees or
  // units is set, and a tree crop must not render as "0.0000 ha".
  test('a tree-counted crop shows trees, not hectares', () => {
    useCycleDetail.mockReturnValue({
      isLoading: false,
      error: null,
      cycle: cycle({ area_ha: null, tree_count: 120, crop_id: 'crop-tree', crop_name: 'Avocado' }),
    })
    render(<OfficerCycleScreen />)

    const detail = screen.getByTestId('cycle-detail')
    expect(detail).toHaveTextContent('120 trees')
    // A hectares FIGURE, not the letters: "Harvest window" and "Shamba" both
    // contain "ha", so a bare substring search fails on correct copy.
    expect(detail).not.toHaveTextContent(/\d+\.\d{4}\s*ha\b/)
  })

  test('supports unit-count cycles and focuses a harvest', () => {
    search.harvest = 'h1'
    useCycleDetail.mockReturnValue({
      isLoading: false,
      error: null,
      cycle: cycle({ area_ha: null, tree_count: null, unit_count: 4, crop_id: 'crop-unit', plot_label: null, season_label: null, planted_on: null, harvest_start: null, harvest_end: null }),
    })
    render(<OfficerCycleScreen />)
    expect(screen.getByTestId('cycle-detail')).toHaveTextContent('4 units')
    expect(screen.getAllByTestId('cycle-harvest')[0]).toHaveAttribute('data-focused', 'true')
  })

  test('shows an unknown measure as a dash', () => {
    useCycleDetail.mockReturnValue({ isLoading: false, error: null, cycle: cycle({ area_ha: null, tree_count: null, unit_count: null, plot_label: null }) })
    render(<OfficerCycleScreen />)
    expect(screen.getByTestId('cycle-detail')).toHaveTextContent('—')
  })

  test('retries cycle read errors', async () => {
    const retry = vi.fn()
    useCycleDetail.mockReturnValue({ isLoading: false, error: new Error('cycle failed'), cycle: null, refetch: retry })
    render(<OfficerCycleScreen />)
    await userEvent.setup().click(screen.getByTestId('error-retry'))
    expect(retry).toHaveBeenCalled()
  })

  test('verification controls call the queue mutation for cycle and harvest', async () => {
    const user = userEvent.setup()
    useCycleDetail.mockReturnValue({ isLoading: false, error: null, cycle: cycle({ verification: 'unverified', harvests: cycle().harvests.map((h) => ({ ...h, verification: 'unverified' })) }) })
    render(<OfficerCycleScreen />)
    await user.click(screen.getByTestId('verify-crop_cycle-c1'))
    await user.click(screen.getByTestId('confirm-dialog-confirm'))
    await user.click(screen.getByTestId('verify-harvest_report-h1'))
    await user.click(screen.getByTestId('confirm-dialog-confirm'))
    await waitFor(() => expect(queueMutate).toHaveBeenCalledTimes(2))
  })

  test('passes pending state through plot, cycle, and harvest controls', () => {
    queueState.isPending = true
    queueState.variables = { table: 'plot', id: 'pl1' }
    useFarmDetail.mockReturnValue({ isLoading: false, error: null, data: farm({ plots: [{ ...farm().plots[0], verification: 'unverified' }] }) })
    render(<OfficerFarmScreen />)

    queueState.variables = { table: 'crop_cycle', id: 'c1' }
    useCycleDetail.mockReturnValue({ isLoading: false, error: null, cycle: cycle({ confidence: null, harvests: cycle().harvests.map((h) => ({ ...h, confidence: null, reported_for: null, verification: 'unverified' })) }) })
    render(<OfficerCycleScreen />)
    queueState.variables = { table: 'harvest_report', id: 'h1' }
    render(<OfficerCycleScreen />)
  })

  test('offers cycle and current-harvest edit controls', () => {
    useCycleDetail.mockReturnValue({ isLoading: false, error: null, cycle: cycle() })
    render(<OfficerCycleScreen />)

    expect(screen.getByTestId('edit-crop_cycle-c1')).toBeInTheDocument()
    expect(screen.getByTestId('edit-harvest_report-h1')).toBeInTheDocument()
    expect(screen.queryByTestId('edit-harvest_report-h2')).not.toBeInTheDocument()
  })

  // A correction stamps captured_at, so it versions the record: a draft typed
  // against an older capture is discarded rather than restored over it.
  test('versions every edit by the record captured_at', () => {
    useFarmDetail.mockReturnValue({
      isLoading: false,
      error: null,
      data: farm({ captured_at: '2026-09-10T08:00:00Z', plots: [{ ...farm().plots[0], captured_at: '2026-09-11T08:00:00Z' }] }),
    })
    useCycleDetail.mockReturnValue({
      isLoading: false,
      error: null,
      cycle: cycle({
        captured_at: '2026-09-12T08:00:00Z',
        harvests: [{ ...cycle().harvests[0], captured_at: '2026-09-13T08:00:00Z' }],
      }),
    })
    render(<OfficerFarmScreen />)
    render(<OfficerCycleScreen />)

    expect(screen.getByTestId('edit-farm-f1')).toHaveAttribute('data-version', '2026-09-10T08:00:00Z')
    expect(screen.getByTestId('edit-plot-pl1')).toHaveAttribute('data-version', '2026-09-11T08:00:00Z')
    expect(screen.getByTestId('edit-crop_cycle-c1')).toHaveAttribute('data-version', '2026-09-12T08:00:00Z')
    expect(screen.getByTestId('edit-harvest_report-h1')).toHaveAttribute('data-version', '2026-09-13T08:00:00Z')
  })
})
