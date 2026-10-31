import { beforeEach, describe, expect, test, vi } from 'vitest'

const upload = vi.fn()
const insert = vi.fn()
vi.mock('@/lib/supabase', () => ({
  supabase: {
    storage: { from: (bucket: string) => ({ upload: (...args: unknown[]) => upload(bucket, ...args) }) },
    from: (table: string) => ({ insert: (row: unknown) => insert(table, row) }),
  },
}))

const { isDraftFiles, plotDocumentPath, uploadPlotDocuments } = await import('@/lib/plotDocuments')

const VILLAGE = '30000000-0000-4000-8000-000000000001'
const PLOT = 'a0000000-0000-4000-8000-000000000001'

const draftFile = (name: string, id = 'f1') => ({
  id,
  name,
  type: 'image/jpeg',
  size: 3,
  blob: new Blob(['abc'], { type: 'image/jpeg' }),
})

beforeEach(() => {
  upload.mockReset()
  insert.mockReset()
  upload.mockResolvedValue({ data: { path: 'x' }, error: null })
  insert.mockResolvedValue({ error: null })
})

describe('plotDocumentPath', () => {
  // The storage policy reads the village from the first folder, and the row's
  // check reads the plot from the second.
  test('files under the village, then the plot', () => {
    expect(plotDocumentPath(VILLAGE, PLOT, 'f1', 'title.jpg')).toBe(`${VILLAGE}/${PLOT}/f1-title.jpg`)
  })

  test('keeps a file name safe for a storage key', () => {
    expect(plotDocumentPath(VILLAGE, PLOT, 'f1', 'Hati ya shamba (1)/é.JPG')).toBe(
      `${VILLAGE}/${PLOT}/f1-Hati-ya-shamba-1-e.JPG`,
    )
  })
})

describe('uploadPlotDocuments', () => {
  test('stores each file, then files a row describing it', async () => {
    const result = await uploadPlotDocuments(VILLAGE, PLOT, [draftFile('title.jpg')])

    expect(upload).toHaveBeenCalledWith(
      'plot-documents',
      `${VILLAGE}/${PLOT}/f1-title.jpg`,
      expect.any(Blob),
      expect.objectContaining({ contentType: 'image/jpeg', upsert: false }),
    )
    expect(insert).toHaveBeenCalledWith('plot_document', {
      plot_id: PLOT,
      village_id: VILLAGE,
      storage_path: `${VILLAGE}/${PLOT}/f1-title.jpg`,
      file_name: 'title.jpg',
      mime_type: 'image/jpeg',
      size_bytes: 3,
    })
    expect(result).toEqual({ filed: ['f1'], failed: [] })
  })

  // A retry after a timeout: the file and the row are already there, which is
  // the outcome wanted, not a failure.
  test('a file already stored and filed counts as filed', async () => {
    upload.mockResolvedValue({ data: null, error: { message: 'The resource already exists', statusCode: '409' } })
    insert.mockResolvedValue({ error: { code: '23505', message: 'duplicate key value' } })

    expect(await uploadPlotDocuments(VILLAGE, PLOT, [draftFile('title.jpg')])).toEqual({
      filed: ['f1'],
      failed: [],
    })
  })

  test('one refused file does not stop the others, and says why', async () => {
    upload
      .mockResolvedValueOnce({ data: null, error: { message: 'mime type text/plain is not supported' } })
      .mockResolvedValueOnce({ data: { path: 'x' }, error: null })

    const result = await uploadPlotDocuments(VILLAGE, PLOT, [draftFile('notes.txt', 'a'), draftFile('title.jpg', 'b')])

    expect(result.filed).toEqual(['b'])
    expect(result.failed).toEqual([{ id: 'a', name: 'notes.txt', message: 'mime type text/plain is not supported' }])
    expect(insert).toHaveBeenCalledTimes(1)
  })
})

describe('isDraftFiles', () => {
  test('accepts a stored list of picked files', () => {
    expect(isDraftFiles([draftFile('title.jpg')])).toBe(true)
    expect(isDraftFiles([])).toBe(true)
  })

  test.each([null, {}, [{ name: 'x' }], [{ ...draftFile('x'), blob: 'not a blob' }]])('rejects %s', (value) => {
    expect(isDraftFiles(value)).toBe(false)
  })
})
