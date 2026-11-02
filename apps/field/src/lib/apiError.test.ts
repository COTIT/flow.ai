import { describe, expect, test } from 'vitest'

import { apiErrorMessage, apiStatus, fieldErrorsOf } from '@/lib/apiError'

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

describe('fieldErrorsOf', () => {
  const refused = failed(422, { message: 'invalid', errors: { quantity: ['too many'], note: ['too long'] } })

  test('texts for keys the form has a field for; nothing left for the banner', () => {
    expect(fieldErrorsOf(refused, ['quantity', 'note'])).toEqual({ fields: { quantity: 'too many', note: 'too long' }, general: null })
  })

  test('a key with no field stays in the general message', () => {
    expect(fieldErrorsOf(refused, ['quantity'])).toEqual({ fields: { quantity: 'too many' }, general: 'too long' })
  })

  test('API keys are mapped to form field names', () => {
    const fieldOf = (key: string) => (key === 'quantity' ? 'qty' : null)
    expect(fieldErrorsOf(refused, fieldOf).fields).toEqual({ qty: 'too many' })
  })

  test('nothing on a field: the error itself is the general message', () => {
    const other = failed(409, { message: 'taken' })
    expect(fieldErrorsOf(other, ['quantity'])).toEqual({ fields: {}, general: other })
    expect(fieldErrorsOf(refused, [])).toEqual({ fields: {}, general: refused })
    expect(fieldErrorsOf(null, ['quantity'])).toEqual({ fields: {}, general: null })
  })
})
