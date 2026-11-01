import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

import { useTourStall } from '@/app/tour/useTourStall'

/**
 * The backstop, and the reason it watches the bubble rather than anything else.
 *
 * The library's overlay renders for lifecycles its tooltip does not, so every
 * way it can stall looks the same to a user: a navy sheet over the page that
 * swallows clicks, with nothing left to explain it. Two such stalls turned up
 * in one afternoon — controlled mode never finishing itself, and a production
 * build that began scrolling to a stop and never drew. Naming them one at a
 * time is a losing game; asking "is anything on the screen" is not.
 */
const settle = () =>
  act(async () => {
    await Promise.resolve()
  })

const bubble = '<div data-testid="tour-tooltip"></div>'
let bubbleRect = new DOMRect(20, 20, 240, 160)

describe('a tour that stops drawing', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    bubbleRect = new DOMRect(20, 20, 240, 160)
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(() => bubbleRect)
    vi.useFakeTimers({ shouldAdvanceTime: true })
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  test('ends, once, after the deadline', async () => {
    const onStall = vi.fn()
    renderHook(() => useTourStall(true, onStall, 10_000))

    await settle()
    expect(onStall).not.toHaveBeenCalled()

    await act(async () => {
      vi.advanceTimersByTime(10_000)
      await Promise.resolve()
    })

    expect(onStall).toHaveBeenCalledTimes(1)
  })

  // An ordinary screen change costs a second or so with no bubble, and must
  // cost nothing else.
  test('a short gap between stops is not a stall', async () => {
    const onStall = vi.fn()
    document.body.innerHTML = bubble
    renderHook(() => useTourStall(true, onStall, 10_000))
    await settle()

    await act(async () => {
      document.body.innerHTML = ''
      vi.advanceTimersByTime(1500)
      await Promise.resolve()
    })
    await act(async () => {
      document.body.innerHTML = bubble
      await Promise.resolve()
      vi.advanceTimersByTime(30_000)
      await Promise.resolve()
    })

    expect(onStall).not.toHaveBeenCalled()
  })

  /**
   * The failure that reached users: a bubble that was drawn, then was not, and
   * never came back. Arming only at the start of a stop missed exactly this.
   */
  test('a bubble that disappears and never returns is a stall', async () => {
    const onStall = vi.fn()
    document.body.innerHTML = bubble
    renderHook(() => useTourStall(true, onStall, 10_000))
    await settle()

    await act(async () => {
      document.body.innerHTML = ''
      await Promise.resolve()
      vi.advanceTimersByTime(10_000)
      await Promise.resolve()
    })

    expect(onStall).toHaveBeenCalledTimes(1)
  })

  test('an off-screen bubble is a stall even though it exists', async () => {
    const onStall = vi.fn()
    bubbleRect = new DOMRect(20, window.innerHeight + 500, 240, 160)
    document.body.innerHTML = bubble
    renderHook(() => useTourStall(true, onStall, 10_000))
    await settle()

    await act(async () => {
      vi.advanceTimersByTime(10_000)
      await Promise.resolve()
    })

    expect(onStall).toHaveBeenCalledTimes(1)
  })

  test('a visually hidden bubble is a stall even though it has geometry', async () => {
    const onStall = vi.fn()
    document.body.innerHTML = '<div style="opacity: 0" data-testid="tour-tooltip"></div>'
    renderHook(() => useTourStall(true, onStall, 10_000))
    await settle()

    await act(async () => {
      vi.advanceTimersByTime(10_000)
      await Promise.resolve()
    })

    expect(onStall).toHaveBeenCalledTimes(1)
  })

  test('and a tour that is not running is not watched', async () => {
    const onStall = vi.fn()
    renderHook(() => useTourStall(false, onStall, 10_000))

    await settle()
    await act(async () => {
      vi.advanceTimersByTime(30_000)
      await Promise.resolve()
    })

    expect(onStall).not.toHaveBeenCalled()
  })

  test('a new callback does not restart the deadline', async () => {
    const onStall = vi.fn()
    const { rerender } = renderHook(
      ({ cb }: { cb: () => void }) => useTourStall(true, cb, 10_000),
      { initialProps: { cb: onStall } },
    )

    await settle()
    await act(async () => {
      vi.advanceTimersByTime(7000)
      await Promise.resolve()
    })
    rerender({ cb: onStall })
    await act(async () => {
      vi.advanceTimersByTime(3000)
      await Promise.resolve()
    })

    expect(onStall).toHaveBeenCalledTimes(1)
  })
})
