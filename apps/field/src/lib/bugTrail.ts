import type { AnyRouter } from '@tanstack/react-router'

/**
 * The last 30 things the user did, attached to a bug report (migration
 * 20261025090001_bug_reports).
 *
 * Clicks, navigations, API calls and uncaught errors. Never a typed value, a
 * request body or a query value: URLs keep their path and query keys only,
 * because a filter or a PostgREST query can carry a name or a phone number.
 */
export type TrailKind = 'click' | 'nav' | 'api' | 'error'

export interface TrailEntry {
  at: string
  kind: TrailKind
  /** Where the user was, redacted like every other URL. */
  route: string
  detail: string
}

const MAX = 30
let trail: TrailEntry[] = []

export function recordTrail(kind: TrailKind, detail: string) {
  trail.push({ at: new Date().toISOString(), kind, route: redactUrl(location.href), detail })
  if (trail.length > MAX) trail = trail.slice(-MAX)
}

export function trailSnapshot(): TrailEntry[] {
  return [...trail]
}

export function clearTrail() {
  trail = []
}

export function redactUrl(url: string): string {
  const parsed = new URL(url, location.origin)
  const keys = [...new Set(parsed.searchParams.keys())]
  return parsed.pathname + (keys.length ? `?${keys.join('&')}` : '')
}

/** What a click hit, as a person would name it: `button "Verify" [verify-button]`. */
function describeTarget(el: Element): string {
  const control = el.closest('[data-testid],button,a,[role],input,select,textarea,label') ?? el
  const role = control.getAttribute('role') ?? control.tagName.toLowerCase()
  const typed = control.matches('input,select,textarea')
  const label = typed
    ? ''
    : (control.getAttribute('aria-label') ?? control.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 60)
  const testId = control.getAttribute('data-testid')
  return [role, label && `"${label}"`, testId && `[${testId}]`].filter(Boolean).join(' ')
}

export function installClickTrail(): () => void {
  const onClick = (event: MouseEvent) => {
    const target = event.target
    if (!(target instanceof Element) || target.closest('[data-bug-report]')) return
    recordTrail('click', describeTarget(target))
  }
  document.addEventListener('click', onClick, true)
  return () => document.removeEventListener('click', onClick, true)
}

/** The Supabase client's fetch: every API call lands on the trail. */
export const trailFetch: typeof fetch = async (input, init) => {
  const method = (init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase()
  const url = redactUrl(input instanceof Request ? input.url : String(input))
  const started = performance.now()
  try {
    const response = await fetch(input, init)
    recordTrail('api', `${method} ${url} → ${response.status} (${Math.round(performance.now() - started)}ms)`)
    return response
  } catch (error) {
    recordTrail('api', `${method} ${url} → ${error instanceof Error ? error.message : String(error)}`)
    throw error
  }
}

let installed = false

/** Once, at start-up: clicks, navigations and uncaught errors. */
export function installBugTrail(router: AnyRouter) {
  if (installed) return
  installed = true
  installClickTrail()
  router.subscribe('onResolved', ({ fromLocation, toLocation }) => {
    recordTrail('nav', `${fromLocation ? redactUrl(fromLocation.href) : '(start)'} → ${redactUrl(toLocation.href)}`)
  })
  window.addEventListener('error', (event) => recordTrail('error', event.message))
  window.addEventListener('unhandledrejection', (event) =>
    recordTrail('error', event.reason instanceof Error ? event.reason.message : String(event.reason)),
  )
}
