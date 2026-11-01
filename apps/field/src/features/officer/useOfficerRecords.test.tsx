import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { act } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const cycleGet = vi.fn()

vi.mock('@genesy/api-client/endpoints/farm-records', () => ({
  farmRecordsCropCyclesGet: (...args: unknown[]) => cycleGet(...args),
  farmRecordsPlotsGet: () => Promise.resolve({ data: { label: 'Kipande cha juu' } }),
  farmRecordsHarvestReportsList: () =>
    Promise.resolve({ data: [], meta: { current_page: 1, last_page: 1, per_page: 100, total: 0, from: null, to: null } }),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { useCycleDetail } = await import('@/features/officer/useOfficerRecords')
const i18n = (await import('@/i18n')).default

/** What the API returns: the cycle with its crop embedded, BOTH names. */
const CYCLE = {
  id: 'cy1',
  crop_id: 'maize',
  crop: { id: 'maize', code: 'maize', name_en: 'Maize', name_sw: 'Mahindi', measured_by: 'area' },
  plot_id: 'p1',
  village_id: 'v1',
  season_label: 'Msimu 2026 A',
  area_ha: '1.6000',
  tree_count: null,
  unit_count: null,
  status: 'growing',
  planted_on: null,
  harvest_start: '2026-09-01',
  harvest_end: '2026-09-30',
  provenance: { verification: 'verified', source: 'field_verified', confidence: 'high', captured_at: '2026-09-09T21:30:00Z' },
}

beforeEach(async () => {
  cycleGet.mockReset()
  cycleGet.mockResolvedValue({ data: CYCLE })
  await i18n.changeLanguage('en')
})

function renderCycle() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return renderHook(() => useCycleDetail('b0000000-0000-4000-8000-000000000001'), {
    wrapper: ({ children }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
  })
}

/**
 * QA #31. The Ilundo officer's locale is `sw`. The verify queue said `Kahawa`
 * and this screen said `Maize`, in the same session, because the crop name was
 * chosen inside the `queryFn` and cached under a key about the CYCLE.
 *
 * i18next initialises to `en` and the stored locale arrives via an effect, so
 * whichever resolved first won — and it was usually English.
 */
describe('useCycleDetail and the active language', () => {
  test('renders the English name in English', async () => {
    const { result } = renderCycle()
    await waitFor(() => expect(result.current.cycle).not.toBeNull())
    expect(result.current.cycle?.crop_name).toBe('Maize')
  })

  test('and the Swahili name in Swahili', async () => {
    await i18n.changeLanguage('sw')
    const { result } = renderCycle()
    await waitFor(() => expect(result.current.cycle).not.toBeNull())
    expect(result.current.cycle?.crop_name).toBe('Mahindi')
  })

  /**
   * The heart of it. The name has to follow the language of the CURRENT
   * render, not the language that happened to be active when the row was
   * fetched.
   */
  test('follows a language change that happens after the fetch', async () => {
    const { result } = renderCycle()
    await waitFor(() => expect(result.current.cycle?.crop_name).toBe('Maize'))

    await act(async () => {
      await i18n.changeLanguage('sw')
    })

    await waitFor(() => expect(result.current.cycle?.crop_name).toBe('Mahindi'))
  })

  // One cache entry, not one per language: the row did not change, only the
  // label on it did.
  test('without going back to the API for it', async () => {
    const { result } = renderCycle()
    await waitFor(() => expect(result.current.cycle?.crop_name).toBe('Maize'))
    expect(cycleGet).toHaveBeenCalledTimes(1)

    await act(async () => {
      await i18n.changeLanguage('sw')
    })
    await waitFor(() => expect(result.current.cycle?.crop_name).toBe('Mahindi'))

    expect(cycleGet).toHaveBeenCalledTimes(1)
  })

  test('and back again', async () => {
    const { result } = renderCycle()
    await waitFor(() => expect(result.current.cycle).not.toBeNull())

    await act(async () => {
      await i18n.changeLanguage('sw')
    })
    await waitFor(() => expect(result.current.cycle?.crop_name).toBe('Mahindi'))

    await act(async () => {
      await i18n.changeLanguage('en')
    })
    await waitFor(() => expect(result.current.cycle?.crop_name).toBe('Maize'))
  })

  // Zero rows is an answer, and it must not become a crash on the way through
  // the render-time mapping.
  test('a missing cycle stays null rather than throwing', async () => {
    cycleGet.mockRejectedValue({ response: { status: 404, data: { message: 'The requested resource could not be found.' } } })
    const { result } = renderCycle()

    await waitFor(() => expect(result.current.isLoading).toBe(false))
    expect(result.current.cycle).toBeNull()
  })
})
