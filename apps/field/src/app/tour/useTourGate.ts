import { useEffect, useState } from 'react'

/**
 * Whether the page has done what a stop asked of the person.
 *
 * A stop can say "pick a village" but cannot check that they did — the page
 * belongs to somebody else's component. What it can see is the result appear,
 * so the gate is an element that only exists once the thing is done.
 *
 * Keyed, like `useTourTarget`, so a new stop reads as closed immediately instead
 * of inheriting the last stop's answer for the one render it takes an effect to
 * correct it.
 */
export function useTourGate(gateKey: string | null, selector: string | null): boolean {
  const [openKey, setOpenKey] = useState<string | null>(null)

  useEffect(() => {
    if (!gateKey || !selector) return

    const check = () => setOpenKey(document.querySelector(selector) ? gateKey : null)

    const observer = new MutationObserver(check)
    observer.observe(document.body, { childList: true, subtree: true })
    queueMicrotask(check)

    return () => observer.disconnect()
  }, [gateKey, selector])

  // A stop with nothing to wait for is open; the state is only about waiting.
  if (!selector) return true
  return gateKey !== null && openKey === gateKey
}
