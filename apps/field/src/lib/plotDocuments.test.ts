import { beforeEach, describe, expect, test, vi } from 'vitest'
import { sessionStore } from '@genesy/api-client/session'

const create = vi.fn()
vi.mock('@genesy/api-client/endpoints/farm-documents', () => ({
  farmDocumentsCreate: (...args: unknown[]) => create(...args),
}))
// fetchPlotDocuments (person detail) still reads Supabase until that screen is ported.
vi.mock('@/lib/supabase', () => ({ supabase: {} }))

const { isDraftFiles, uploadPlotDocuments } = await import('@/lib/plotDocuments')

const WORKSPACE = '20000000-0000-4000-8000-000000000001'
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
  create.mockReset()
  create.mockResolvedValue({ data: { id: 'f1' } })
  sessionStore.getState().signIn({ access_token: 'tok', token_type: 'Bearer', workspace_ids: [WORKSPACE] })
})

describe('uploadPlotDocuments', () => {
  // The id is made when the file is picked: a retry after a timeout sends it again, and the API
  // answers 200 with the document already filed instead of filing a second one.
  test('files each one against the plot, named, with its device id', async () => {
    const result = await uploadPlotDocuments(VILLAGE, PLOT, [draftFile('title.jpg')])

    expect(create).toHaveBeenCalledWith(WORKSPACE, PLOT, { file: expect.any(File), id: 'f1' })
    const sent = create.mock.calls[0][2].file as File
    expect(sent.name).toBe('title.jpg')
    expect(sent.type).toBe('image/jpeg')
    expect(result).toEqual({ filed: ['f1'], failed: [] })
  })

  test('one refused file does not stop the others, and says why', async () => {
    create
      .mockRejectedValueOnce({
        message: 'Request failed with status code 422',
        response: { status: 422, data: { message: 'invalid', errors: { file: ['this file type is not accepted'] } } },
      })
      .mockResolvedValueOnce({ data: { id: 'b' } })

    const result = await uploadPlotDocuments(VILLAGE, PLOT, [draftFile('notes.txt', 'a'), draftFile('title.jpg', 'b')])

    expect(result.filed).toEqual(['b'])
    expect(result.failed).toEqual([{ id: 'a', name: 'notes.txt', message: 'this file type is not accepted' }])
  })

  test('a file that never reached the API says so, and stays to retry', async () => {
    create.mockRejectedValueOnce(new Error('Network Error'))

    const result = await uploadPlotDocuments(VILLAGE, PLOT, [draftFile('title.jpg')])

    expect(result).toEqual({ filed: [], failed: [{ id: 'f1', name: 'title.jpg', message: 'Network Error' }] })
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
