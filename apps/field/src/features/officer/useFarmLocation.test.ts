import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'

import { useFarmLocation } from '@/features/officer/useFarmLocation'

type Success = (position: GeolocationPosition) => void
type Failure = (error: GeolocationPositionError) => void

function mockGeolocation() {
  const getCurrentPosition = vi.fn<
    (success: Success, failure?: Failure) => void
  >()
  Object.defineProperty(navigator, 'geolocation', {
    value: { getCurrentPosition },
    configurable: true,
  })
  return getCurrentPosition
}

function position(latitude: number, longitude: number): GeolocationPosition {
  return { coords: { latitude, longitude } } as GeolocationPosition
}

function error(code: number): GeolocationPositionError {
  return { code, message: '' } as GeolocationPositionError
}

describe('useFarmLocation', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    Reflect.deleteProperty(navigator, 'geolocation')
  })

  test('reports unsupported when the browser has no geolocation, and never calls it', () => {
    Reflect.deleteProperty(navigator, 'geolocation')
    const { result } = renderHook(() => useFarmLocation(true))

    expect(result.current.status).toBe('unsupported')
    expect(result.current.coords).toBeNull()
  })

  test('stays idle, and never calls the browser, while not enabled', () => {
    const getCurrentPosition = mockGeolocation()
    const { result } = renderHook(() => useFarmLocation(false))

    expect(result.current.status).toBe('idle')
    expect(getCurrentPosition).not.toHaveBeenCalled()
  })

  test('attempts as soon as enabled turns true, and rounds a successful read to 6 decimal places', async () => {
    const getCurrentPosition = mockGeolocation()
    const { result, rerender } = renderHook(({ enabled }) => useFarmLocation(enabled), {
      initialProps: { enabled: false },
    })
    expect(getCurrentPosition).not.toHaveBeenCalled()

    rerender({ enabled: true })
    expect(result.current.status).toBe('acquiring')
    expect(getCurrentPosition).toHaveBeenCalledTimes(1)

    const [success] = getCurrentPosition.mock.calls[0]
    act(() => success(position(-7.123456789, 34.987654321)))

    await waitFor(() => expect(result.current.status).toBe('acquired'))
    expect(result.current.coords).toEqual({ latitude: '-7.123457', longitude: '34.987654' })
  })

  test('never auto-attempts a second time once enabled, even across rerenders', () => {
    const getCurrentPosition = mockGeolocation()
    const { rerender } = renderHook(({ enabled }) => useFarmLocation(enabled), {
      initialProps: { enabled: true },
    })
    expect(getCurrentPosition).toHaveBeenCalledTimes(1)

    rerender({ enabled: true })
    rerender({ enabled: true })
    expect(getCurrentPosition).toHaveBeenCalledTimes(1)
  })

  test('maps a permission denial to "denied"', async () => {
    const getCurrentPosition = mockGeolocation()
    const { result } = renderHook(() => useFarmLocation(true))

    const [, failure] = getCurrentPosition.mock.calls[0]
    act(() => failure?.(error(1)))

    await waitFor(() => expect(result.current.status).toBe('denied'))
    expect(result.current.coords).toBeNull()
  })

  test('maps any other failure (timeout, position unavailable) to "error"', async () => {
    const getCurrentPosition = mockGeolocation()
    const { result } = renderHook(() => useFarmLocation(true))

    const [, failure] = getCurrentPosition.mock.calls[0]
    act(() => failure?.(error(3)))

    await waitFor(() => expect(result.current.status).toBe('error'))
  })

  test('retry re-attempts after a failure, and can succeed', async () => {
    const getCurrentPosition = mockGeolocation()
    const { result } = renderHook(() => useFarmLocation(true))

    const [, firstFailure] = getCurrentPosition.mock.calls[0]
    act(() => firstFailure?.(error(2)))
    await waitFor(() => expect(result.current.status).toBe('error'))

    act(() => result.current.retry())
    expect(result.current.status).toBe('acquiring')
    expect(getCurrentPosition).toHaveBeenCalledTimes(2)

    const [secondSuccess] = getCurrentPosition.mock.calls[1]
    act(() => secondSuccess(position(1, 2)))

    await waitFor(() => expect(result.current.status).toBe('acquired'))
    expect(result.current.coords).toEqual({ latitude: '1.000000', longitude: '2.000000' })
  })

  test('a stale reply from a superseded attempt is ignored', async () => {
    const getCurrentPosition = mockGeolocation()
    const { result } = renderHook(() => useFarmLocation(true))

    const [, firstFailure] = getCurrentPosition.mock.calls[0]
    act(() => firstFailure?.(error(2)))
    await waitFor(() => expect(result.current.status).toBe('error'))

    act(() => result.current.retry())
    const [firstSuccess] = getCurrentPosition.mock.calls[0]
    // The superseded first call's success arrives after the retry started.
    act(() => firstSuccess(position(9, 9)))

    expect(result.current.status).toBe('acquiring')
    expect(result.current.coords).toBeNull()
  })
})
