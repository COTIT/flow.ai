import { useEffect, useRef } from 'react'

import { isVisibleInViewport } from '@/app/tour/tourVisibility'

/**
 * A tour that is running but drawing nothing has stopped being a tour.
 *
 * This is the backstop, and it exists because the library has more ways to
 * stall than are worth enumerating. The one that reached users: the overlay is
 * a full-screen `<path fill="var(--scrim)">` with `pointer-events: auto`, and
 * it renders for lifecycles the TOOLTIP does not — so any stall leaves a navy
 * sheet over the page swallowing every click, with nothing left to explain it.
 * Reported from the demo as "the screen stays blue and I cannot click
 * anything", on desktop and mobile, escapable only by reloading into it again.
 *
 * Two stalls were found in one afternoon: in controlled mode the library never
 * sets `finished` by itself, and on a production build it began scrolling to a
 * stop, never reported finishing, and never drew. Rather than keep naming
 * them, this watches the only thing that actually matters — whether a bubble
 * is on the screen — and ends the tour when one has not been for `ms`.
 *
 * A `MutationObserver` rather than polling, and the timer restarts only on the
 * edge into absence, so an ordinary screen change (a second or so with no
 * bubble) costs nothing.
 */
export function useTourStall(active: boolean, onStall: () => void, ms = 10_000): void {
  // Held in a ref so a new callback identity does not restart the deadline.
  const stalled = useRef(onStall)
  useEffect(() => {
    stalled.current = onStall
  }, [onStall])

  useEffect(() => {
    if (!active) return
    if (typeof MutationObserver === 'undefined') return

    let timer: ReturnType<typeof setTimeout> | undefined

    const evaluate = () => {
      const drawing = isVisibleInViewport(document.querySelector('[data-testid="tour-tooltip"]'))
      if (drawing) {
        clearTimeout(timer)
        timer = undefined
        return
      }
      if (timer) return
      timer = setTimeout(() => stalled.current(), ms)
    }

    const observer = new MutationObserver(evaluate)
    observer.observe(document.body, {
      attributes: true,
      attributeFilter: ['class', 'hidden', 'style'],
      childList: true,
      subtree: true,
    })
    const visualViewport = window.visualViewport
    window.addEventListener('scroll', evaluate, true)
    window.addEventListener('resize', evaluate)
    visualViewport?.addEventListener('scroll', evaluate)
    visualViewport?.addEventListener('resize', evaluate)
    // Asynchronously, so no state settles inside the effect's own turn.
    queueMicrotask(evaluate)

    return () => {
      observer.disconnect()
      window.removeEventListener('scroll', evaluate, true)
      window.removeEventListener('resize', evaluate)
      visualViewport?.removeEventListener('scroll', evaluate)
      visualViewport?.removeEventListener('resize', evaluate)
      clearTimeout(timer)
    }
  }, [active, ms])
}
