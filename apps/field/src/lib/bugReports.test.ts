import { beforeEach, expect, test, vi } from 'vitest'

const upload = vi.fn()
const insert = vi.fn()
vi.mock('@/lib/supabase', () => ({
  supabase: {
    storage: { from: (bucket: string) => ({ upload: (...args: unknown[]) => upload(bucket, ...args) }) },
    from: (table: string) => ({ insert: (row: unknown) => insert(table, row) }),
  },
}))

const { submitBugReport } = await import('@/lib/bugReports')

const ME = '80000000-0000-4000-8000-000000000005'
const actions = [{ at: '2026-10-25T10:00:00Z', kind: 'nav' as const, route: '/farm', detail: '(start) → /farm' }]
const report = {
  reporterId: ME,
  description: 'The survey list is empty',
  actions,
  screenshot: new Blob(['png'], { type: 'image/png' }),
  photo: null,
}

beforeEach(() => {
  upload.mockReset().mockResolvedValue({ data: {}, error: null })
  insert.mockReset().mockResolvedValue({ error: null })
})

test('stores the image in the reporter folder, then files the report pointing at it', async () => {
  await submitBugReport(report)

  const [bucket, path, , options] = upload.mock.calls[0]
  expect(bucket).toBe('bug-reports')
  expect(path).toMatch(new RegExp(`^${ME}/[0-9a-f-]{36}\\.png$`))
  expect(options).toMatchObject({ contentType: 'image/png', upsert: false })

  const [table, row] = insert.mock.calls[0]
  expect(table).toBe('bug_report')
  expect(row).toMatchObject({
    description: 'The survey list is empty',
    actions,
    screenshot_path: path,
    photo_path: null,
    route: '/',
  })
  expect(row.viewport).toMatch(/^\d+x\d+$/)
})

test('stores a photo as well, keeping its own extension', async () => {
  await submitBugReport({ ...report, photo: new File(['jpg'], 'IMG_1.JPG', { type: 'image/jpeg' }) })
  expect(upload).toHaveBeenCalledTimes(2)
  expect(insert.mock.calls[0][1].photo_path).toMatch(/\.jpg$/)
})

test('a refused upload stops before the report is filed, with storage\'s own message', async () => {
  upload.mockResolvedValue({ data: null, error: { message: 'The object exceeded the maximum allowed size' } })
  await expect(submitBugReport(report)).rejects.toThrow('The object exceeded the maximum allowed size')
  expect(insert).not.toHaveBeenCalled()
})

test('a refused report surfaces the database message', async () => {
  insert.mockResolvedValue({ error: { message: 'new row violates check constraint "bug_report_has_image"' } })
  await expect(submitBugReport(report)).rejects.toThrow('bug_report_has_image')
})
