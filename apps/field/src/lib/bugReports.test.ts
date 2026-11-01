import { beforeEach, expect, test, vi } from 'vitest'

const farmBugReportsCreate = vi.fn()
vi.mock('@genesy/api-client/endpoints/farm-bug-reports', () => ({
  farmBugReportsCreate: (...args: unknown[]) => farmBugReportsCreate(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { submitBugReport } = await import('@/lib/bugReports')

const ID = '81000000-0000-4000-8000-000000000001'
const actions = [{ at: '2026-10-25T10:00:00Z', kind: 'nav' as const, route: '/farm', detail: '(start) → /farm' }]
const screenshot = new Blob(['png'], { type: 'image/png' })
const report = { id: ID, description: 'The survey list is empty', actions, screenshot, photo: null }

beforeEach(() => {
  farmBugReportsCreate.mockReset().mockResolvedValue({ data: { id: ID, ref: 12 } })
})

test('files one multipart report under the caller, with the client id and the trail as JSON', async () => {
  await submitBugReport(report)

  expect(farmBugReportsCreate).toHaveBeenCalledTimes(1)
  const [workspace, form] = farmBugReportsCreate.mock.calls[0]
  expect(workspace).toBe('w1')
  expect(form).toEqual({
    id: ID,
    route: '/',
    user_agent: navigator.userAgent,
    viewport: expect.stringMatching(/^\d+x\d+$/),
    description: 'The survey list is empty',
    actions: JSON.stringify(actions),
    screenshot,
    photo: null,
  })
  // The API files under the signed-in user; naming a reporter is an unknown field (422).
  expect(form).not.toHaveProperty('reporter_id')
})

test('sends a photo as well', async () => {
  const photo = new File(['jpg'], 'IMG_1.JPG', { type: 'image/jpeg' })
  await submitBugReport({ ...report, photo })
  expect(farmBugReportsCreate.mock.calls[0][1].photo).toBe(photo)
})

test('a refusal comes back as the API error', async () => {
  const refusal = { isAxiosError: true, response: { status: 422, data: { message: 'The upload has an unknown field.' } } }
  farmBugReportsCreate.mockRejectedValue(refusal)
  await expect(submitBugReport(report)).rejects.toBe(refusal)
})
