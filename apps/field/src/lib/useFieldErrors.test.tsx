import { renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

import '@/i18n'
import { useFieldErrors } from '@/lib/useFieldErrors'

const refused = (errors: Record<string, string[]>) => ({ isAxiosError: true, response: { status: 422, data: { message: 'invalid', errors } } })

describe('useFieldErrors', () => {
  test('puts each text on its field, focuses the first, and leaves the rest for the banner', () => {
    const setError = vi.fn()
    const error = refused({ quantity: ['must be at most 10'], equipment_id: ['unknown machine'] })
    const { result } = renderHook(() => useFieldErrors(error, setError, ['quantity']))

    expect(setError).toHaveBeenCalledTimes(1)
    expect(setError).toHaveBeenCalledWith('quantity', { type: 'server', message: 'must be at most 10' }, { shouldFocus: true })
    expect(result.current).toBe('unknown machine')
  })

  test('machine noise is humanised like any other API text', () => {
    const setError = vi.fn()
    const error = refused({ quantity: ['new row violates check constraint "pue_request_quantity_check"'] })
    renderHook(() => useFieldErrors(error, setError, ['quantity']))
    expect(setError.mock.calls[0][1].message).not.toMatch(/constraint/)
  })

  test('sets a refusal once, however often the screen renders', () => {
    const setError = vi.fn()
    const error = refused({ quantity: ['too many'] })
    const { rerender, result } = renderHook(() => useFieldErrors(error, setError, ['quantity']))
    rerender()
    expect(setError).toHaveBeenCalledTimes(1)
    expect(result.current).toBeNull()
  })

  test('focuses by element id when the inputs are not registered', () => {
    const input = document.createElement('input')
    input.id = 'qty'
    document.body.append(input)
    const setError = vi.fn()
    renderHook(() => useFieldErrors(refused({ quantity: ['too many'] }), setError, ['quantity'], () => 'qty'))
    expect(setError).toHaveBeenCalledWith('quantity', expect.anything(), { shouldFocus: false })
    expect(document.activeElement).toBe(input)
    input.remove()
  })

  test('another failure goes to the banner whole', () => {
    const setError = vi.fn()
    const error = { isAxiosError: true, response: { status: 409, data: { message: 'taken' } } }
    const { result } = renderHook(() => useFieldErrors(error, setError, ['quantity']))
    expect(setError).not.toHaveBeenCalled()
    expect(result.current).toBe(error)
  })
})
