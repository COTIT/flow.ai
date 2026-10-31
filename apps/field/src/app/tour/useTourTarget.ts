import { useEffect, useRef, useState } from 'react'

import { isVisibleInViewport, scrollTourTargetIntoView } from '@/app/tour/tourVisibility'

/**
 * How many times a scroll that is not ours may be undone before the target is
 * left to the deadline. A target that can never be brought into view must not
 * turn every scroll event into another scroll.
 */
const MAX_RESCROLLS = 6

/**
 * Prepare one tour target before Joyride is allowed to mount its overlay.
 *
 * Joyride's own animated scroll is disabled because it has stalled in a
 * production build. This hook keeps that workaround, waits for route content
 * to exist, and performs a synchronous browser scroll instead. The keyed
 * result makes a new step read as unready immediately, without an effect that
 * first has to reset stale state from the previous stop.
 */
export function useTourTarget(
  targetKey: string | null,
  selector: string | null,
  routeReady: boolean,
  onMissing: () => void,
  timeoutMs = 10_000,
): boolean {
  const [readyKey, setReadyKey] = useState<string | null>(null)
  const missing = useRef(onMissing)

  useEffect(() => {
    missing.current = onMissing
  }, [onMissing])

  useEffect(() => {
    if (!targetKey || !selector) return

    let done = false
    let prepared: Element | null = null

    const visualViewport = window.visualViewport

    let rescrolls = 0

    const clean = () => {
      observer.disconnect()
      clearTimeout(timer)
      window.removeEventListener('scroll', onViewportChange, true)
      window.removeEventListener('resize', onViewportChange)
      visualViewport?.removeEventListener('scroll', onViewportChange)
      visualViewport?.removeEventListener('resize', onViewportChange)
    }

    const finish = () => {
      if (done) return
      done = true
      clean()
      setReadyKey(targetKey)
    }

    // A plain boolean: the library's type guard would narrow `prepared` to
    // `null` in the branch below, where it is exactly what we need to read.
    const inView = (element: Element | null): boolean => isVisibleInViewport(element)

    function verify(afterOutsideMove: boolean) {
      if (done) return
      if (inView(prepared)) {
        finish()
        return
      }
      // The router puts the page back at the top after a navigation, and does it
      // AFTER the target was scrolled into view: the anchor is on the page,
      // below the fold, and nothing further changes in the DOM to make us look
      // again. A scroll or resize we did not ask for is the cue to put it back.
      if (afterOutsideMove && prepared?.isConnected && rescrolls < MAX_RESCROLLS) {
        rescrolls += 1
        scrollTourTargetIntoView(prepared)
      }
    }

    function onViewportChange() {
      verify(true)
    }

    const prepare = () => {
      if (done || !routeReady) return

      const target = document.querySelector(selector)
      if (!target) return

      if (target !== prepared) {
        prepared = target
        scrollTourTargetIntoView(target)
      }

      // The automatic scroll is synchronous, but defer the geometry read until
      // the browser has applied the resulting layout and sticky positioning.
      queueMicrotask(() => verify(false))
    }

    const observer = new MutationObserver(prepare)
    observer.observe(document.body, { childList: true, subtree: true })
    window.addEventListener('scroll', onViewportChange, true)
    window.addEventListener('resize', onViewportChange)
    visualViewport?.addEventListener('scroll', onViewportChange)
    visualViewport?.addEventListener('resize', onViewportChange)

    const timer = setTimeout(() => {
      if (done) return
      done = true
      clean()
      missing.current()
    }, timeoutMs)

    queueMicrotask(prepare)

    return () => {
      done = true
      clean()
    }
  }, [routeReady, selector, targetKey, timeoutMs])

  return targetKey !== null && readyKey === targetKey
}
