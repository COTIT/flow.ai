import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const usePlotDocuments = vi.fn()
const mutate = vi.fn()
const uploadState = { isPending: false, data: undefined as unknown, error: null as Error | null, isError: false }
vi.mock('@/lib/plotDocuments', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/plotDocuments')>()),
  usePlotDocuments: (id: string) => usePlotDocuments(id),
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
  storage_path: `${VILLAGE}/${PLOT}/f1-title.jpg`,
  url: 'https://signed.example/title.jpg',
  ...over,
})

beforeEach(() => {
  usePlotDocuments.mockReset()
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
  test('lists each document as a link the verifier can open', () => {
    usePlotDocuments.mockReturnValue({ isLoading: false, error: null, data: [doc(), doc({ id: 'd2', file_name: 'map.pdf', mime_type: 'application/pdf' })] })
    render(<PlotDocuments plotId={PLOT} villageId={VILLAGE} />)

    const links = screen.getAllByTestId('plot-document')
    expect(links).toHaveLength(2)
    expect(links[0]).toHaveAttribute('href', 'https://signed.example/title.jpg')
    expect(links[0]).toHaveAttribute('target', '_blank')
    expect(screen.getByTestId(`plot-documents-${PLOT}`)).toHaveTextContent('Title documents (2)')
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
})
