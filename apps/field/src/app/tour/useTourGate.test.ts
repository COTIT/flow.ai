import { act, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'

import { useTourGate } from '@/app/tour/useTourGate'

afterEach(() => {
  document.body.innerHTML = ''
})

const settle = () =>
  act(async () => {
    await Promise.resolve()
    await Promise.resolve()
  })

/**
 * Some stops ask the person to do something before going on: pick a village,
 * click a voucher. The stop cannot check that they did — the page belongs to
 * somebody else's component — but it can see the result appear.
 */
describe('a stop that waits for the page to change', () => {
  test('a stop with no gate is open', () => {
    const { result } = renderHook(() => useTourGate('step-1', null))
    expect(result.current).toBe(true)
  })

  test('is closed until the element it waits for is on the page', async () => {
    const { result } = renderHook(() => useTourGate('step-1', '[data-testid="tile-production"]'))
    await settle()
    expect(result.current).toBe(false)

    await act(async () => {
      const el = document.createElement('div')
      el.dataset.testid = 'tile-production'
      document.body.append(el)
      await Promise.resolve()
    })
    await settle()

    expect(result.current).toBe(true)
  })

  test('is open at once if the element is already there', async () => {
    const el = document.createElement('div')
    el.dataset.testid = 'tile-production'
    document.body.append(el)

    const { result } = renderHook(() => useTourGate('step-1', '[data-testid="tile-production"]'))
    await settle()

    expect(result.current).toBe(true)
  })

  // The same selector can gate a stop twice, and the second time round the
  // person has not done the thing yet.
  test('a different stop starts closed again, even for the same element', async () => {
    const el = document.createElement('div')
    el.dataset.testid = 'tile-production'
    document.body.append(el)

    const { result, rerender } = renderHook(
      ({ id }) => useTourGate(id, '[data-testid="tile-production"]'),
      { initialProps: { id: 'step-1' } },
    )
    await settle()
    expect(result.current).toBe(true)

    el.remove()
    rerender({ id: 'step-2' })
    expect(result.current).toBe(false)
  })

  test('closes again if the element goes away', async () => {
    const el = document.createElement('div')
    el.dataset.testid = 'tile-production'
    document.body.append(el)

    const { result } = renderHook(() => useTourGate('step-1', '[data-testid="tile-production"]'))
    await settle()
    expect(result.current).toBe(true)

    await act(async () => {
      el.remove()
      await Promise.resolve()
    })
    await settle()

    expect(result.current).toBe(false)
  })
})
