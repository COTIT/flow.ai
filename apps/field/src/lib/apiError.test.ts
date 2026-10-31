import { describe, expect, test } from 'vitest'

import { apiErrorMessage, apiStatus } from '@/lib/apiError'

const failed = (status: number, data: unknown) => ({ response: { status, data } })

describe('apiError', () => {
  test('a field error beats the summary message', () => {
    const error = failed(422, { message: 'invalid', errors: { password: ['too short'] } })
    expect(apiErrorMessage(error)).toBe('too short')
    expect(apiStatus(error)).toBe(422)
  })

  test('falls back to the message, then to null', () => {
    expect(apiErrorMessage(failed(403, { message: 'nope' }))).toBe('nope')
    expect(apiErrorMessage(new Error('offline'))).toBeNull()
    expect(apiStatus(new Error('offline'))).toBeUndefined()
  })
})
