import { afterEach, beforeEach, expect, test, vi } from 'vitest'

import { createApiClient } from '@genesy/api-client/client'

import { clearTrail, installClickTrail, recordTrail, redactUrl, trailApi, trailSnapshot } from '@/lib/bugTrail'

beforeEach(() => clearTrail())
afterEach(() => {
  document.body.innerHTML = ''
  vi.unstubAllGlobals()
})

test('keeps only the last 30 entries, oldest first', () => {
  for (let i = 1; i <= 35; i++) recordTrail('nav', `step ${i}`)
  const trail = trailSnapshot()
  expect(trail).toHaveLength(30)
  expect(trail[0].detail).toBe('step 6')
  expect(trail[29].detail).toBe('step 35')
})

test('a snapshot does not move when more is recorded', () => {
  recordTrail('nav', 'a')
  const frozen = trailSnapshot()
  recordTrail('nav', 'b')
  expect(frozen).toHaveLength(1)
})

test('a URL keeps its path and query keys, never the values', () => {
  expect(redactUrl('https://x.supabase.co/rest/v1/person?select=*&phone=eq.0712345678')).toBe(
    '/rest/v1/person?select&phone',
  )
  expect(redactUrl('/farm/surveys')).toBe('/farm/surveys')
})

test('a click names the control by test id and label, never a typed value', () => {
  const stop = installClickTrail()
  document.body.innerHTML = `
    <button data-testid="verify-button"><span>Verify</span></button>
    <input data-testid="phone" value="0712345678" />`
  document.querySelector('span')!.click()
  document.querySelector('input')!.click()
  stop()
  const [button, input] = trailSnapshot()
  expect(button).toMatchObject({ kind: 'click', detail: 'button "Verify" [verify-button]' })
  expect(input.detail).toBe('input [phone]')
  expect(JSON.stringify(trailSnapshot())).not.toContain('0712345678')
})

test('clicks inside the bug reporter itself are not recorded', () => {
  const stop = installClickTrail()
  document.body.innerHTML = '<div data-bug-report><button>Send</button></div>'
  document.querySelector('button')!.click()
  stop()
  expect(trailSnapshot()).toHaveLength(0)
})

/** An axios instance like the app's: `/v1` base, answering with `reply` instead of the network. */
function instance(reply: (config: { url?: string }) => Promise<unknown>) {
  const api = createApiClient({ baseURL: '/v1' })
  api.defaults.adapter = reply as never
  trailApi(api)
  return api
}

test('an API call is recorded with method, redacted path, status and time — never a body', async () => {
  const api = instance(async (config) => ({ data: {}, status: 201, statusText: 'Created', headers: {}, config }))
  await api.post('/workspace/w1/farm/bug-reports', { description: 'secret' }, { params: { phone: '0712345678' } })
  const [entry] = trailSnapshot()
  expect(entry.kind).toBe('api')
  expect(entry.detail).toMatch(/^POST \/v1\/workspace\/w1\/farm\/bug-reports\?phone → 201 \(\d+ms\)$/)
  expect(entry.detail).not.toContain('secret')
  expect(entry.detail).not.toContain('0712345678')
})

test('a refused API call is recorded with its status and still fails', async () => {
  const api = instance(async (config) => {
    throw Object.assign(new Error('Request failed'), { config, response: { status: 422, data: { message: 'secret' } } })
  })
  await expect(api.get('/farm/me')).rejects.toThrow()
  expect(trailSnapshot()[0].detail).toMatch(/^GET \/v1\/farm\/me → 422 \(\d+ms\)$/)
})

test('a call that never got an answer is recorded with the reason', async () => {
  const api = instance(async (config) => {
    throw Object.assign(new Error('Network Error'), { config })
  })
  await expect(api.get('/farm/me')).rejects.toThrow()
  expect(trailSnapshot()[0].detail).toMatch(/^GET \/v1\/farm\/me → Network Error/)
})
