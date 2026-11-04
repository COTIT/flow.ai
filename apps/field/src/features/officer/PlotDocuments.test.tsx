import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const usePlotDocuments = vi.fn()
const openPlotDocument = vi.fn()
const mutate = vi.fn()
const uploadState = { isPending: false, data: undefined as unknown, error: null as Error | null, isError: false }
vi.mock('@/lib/plotDocuments', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/plotDocuments')>()),
  usePlotDocuments: (id: string) => usePlotDocuments(id),
  openPlotDocument: (...args: unknown[]) => openPlotDocument(...args),
  useUploadPlotDocuments: () => ({ mutate, ...uploadState }),
}))

const { PlotDocuments } = await import('@/features/officer/PlotDocuments')
await import('@/i18n')

const PLOT = 'a0000000-0000-4000-8000-000000000001'
const VILLAGE = '30000000-0000-4000-8000-000000000001'

const doc = (over: Record<string, unknown> = {}) => ({
  id: 'd1',
  file_name: 'title.jpg',
  mime_type: 'image/jpeg',
  size_bytes: 2048,
  uploaded_at: '2026-10-05T10:00:00Z',
  ...over,
})

beforeEach(() => {
  usePlotDocuments.mockReset()
  openPlotDocument.mockReset()
  mutate.mockReset()
  uploadState.isPending = false
  uploadState.data = undefined
  uploadState.error = null
  uploadState.isError = false
})

/**
 * The title documents filed against a plot, where the verifier checks the
 * plot's fields against them. Staff only — RLS decides that, not this.
 */
describe('PlotDocuments', () => {
  // The tab opens on the tap (so no popup blocker stops it) and goes to the API's short-lived link.
  test('lists each document, and opening one loads its download link in a new tab', async () => {
    const tab = { opener: {} as unknown, location: { href: '' }, close: vi.fn() }
    const open = vi.spyOn(window, 'open').mockReturnValue(tab as unknown as Window)
    openPlotDocument.mockResolvedValue('https://files.example/title.jpg?sig=1')
    usePlotDocuments.mockReturnValue({ isLoading: false, error: null, data: [doc(), doc({ id: 'd2', file_name: 'map.pdf', mime_type: 'application/pdf' })] })
    render(<PlotDocuments plotId={PLOT} villageId={VILLAGE} />)

    const links = screen.getAllByTestId('plot-document')
    expect(links).toHaveLength(2)
    expect(screen.getByTestId(`plot-documents-${PLOT}`)).toHaveTextContent('Title documents (2)')
    fireEvent.click(links[0])
    expect(open).toHaveBeenCalledWith('', '_blank')
    await waitFor(() => expect(tab.location.href).toBe('https://files.example/title.jpg?sig=1'))
    expect(openPlotDocument).toHaveBeenCalledWith(PLOT, 'd1')
    expect(tab.opener).toBeNull()
    open.mockRestore()
  })

  test('a link the API refuses closes the tab and says why', async () => {
    const tab = { opener: null, location: { href: '' }, close: vi.fn() }
    const open = vi.spyOn(window, 'open').mockReturnValue(tab as unknown as Window)
    openPlotDocument.mockRejectedValue({ response: { status: 503, data: { message: 'storage is unavailable' } } })
    usePlotDocuments.mockReturnValue({ isLoading: false, error: null, data: [doc()] })
    render(<PlotDocuments plotId={PLOT} villageId={VILLAGE} />)

    fireEvent.click(screen.getByTestId('plot-document'))
    expect(await screen.findByTestId('plot-document-open-error')).toHaveTextContent('storage is unavailable')
    expect(tab.close).toHaveBeenCalled()
    open.mockRestore()
  })

  test('with popups blocked the link opens in this tab', async () => {
    const open = vi.spyOn(window, 'open').mockReturnValue(null)
    const assign = vi.fn()
    vi.stubGlobal('location', { ...window.location, assign })
    openPlotDocument.mockResolvedValue('https://files.example/title.jpg?sig=1')
    usePlotDocuments.mockReturnValue({ isLoading: false, error: null, data: [doc()] })
    render(<PlotDocuments plotId={PLOT} villageId={VILLAGE} />)

    fireEvent.click(screen.getByTestId('plot-document'))
    await waitFor(() => expect(assign).toHaveBeenCalledWith('https://files.example/title.jpg?sig=1'))
    vi.unstubAllGlobals()
    open.mockRestore()
  })

  // Zero rows is an answer, not an error.
  test('none filed is said plainly', () => {
    usePlotDocuments.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<PlotDocuments plotId={PLOT} villageId={VILLAGE} />)

    expect(screen.getByTestId('plot-documents-empty')).toBeInTheDocument()
  })

  test('says only staff see them', () => {
    usePlotDocuments.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<PlotDocuments plotId={PLOT} villageId={VILLAGE} />)

    expect(screen.getByTestId(`plot-documents-${PLOT}`)).toHaveTextContent(/staff only/i)
  })

  test('adding documents files them against this plot', () => {
    usePlotDocuments.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<PlotDocuments plotId={PLOT} villageId={VILLAGE} />)

    const file = new File(['abc'], 'deed.jpg', { type: 'image/jpeg' })
    fireEvent.change(screen.getByTestId(`plot-documents-add-${PLOT}`), { target: { files: [file] } })

    expect(mutate).toHaveBeenCalledWith([expect.objectContaining({ name: 'deed.jpg', type: 'image/jpeg', size: 3 })])
  })

  test('a file storage refused is named with its reason', () => {
    usePlotDocuments.mockReturnValue({ isLoading: false, error: null, data: [] })
    uploadState.data = { filed: [], failed: [{ id: 'x', name: 'notes.txt', message: 'mime type text/plain is not supported' }] }
    render(<PlotDocuments plotId={PLOT} villageId={VILLAGE} />)

    expect(screen.getByTestId('plot-documents-failed')).toHaveTextContent('notes.txt')
    expect(screen.getByTestId('plot-documents-failed')).toHaveTextContent('not supported')
  })

  // axios's own English for a dropped connection is not a reason a user can read.
  test('a file lost to the connection says so in words, keeping its name', () => {
    usePlotDocuments.mockReturnValue({ isLoading: false, error: null, data: [] })
    uploadState.data = { filed: [], failed: [{ id: 'x', name: 'deed.jpg', message: 'Network Error' }] }
    render(<PlotDocuments plotId={PLOT} villageId={VILLAGE} />)

    const failed = screen.getByTestId('plot-documents-failed')
    expect(failed).toHaveTextContent('deed.jpg')
    expect(failed).not.toHaveTextContent('Network Error')
  })
})
