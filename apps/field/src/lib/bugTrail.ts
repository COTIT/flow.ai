import type { AnyRouter } from '@tanstack/react-router'

import { api } from '@/lib/api'

/**
 * The last 30 things the user did, attached to a bug report (migration
 * 20261025090001_bug_reports).
 *
 * Clicks, navigations, API calls and uncaught errors. Never a typed value, a
 * request body or a query value: URLs keep their path and query keys only,
 * because a filter or a query can carry a name or a phone number.
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

/** Every Genesy API call lands on the trail: method, redacted path, status, time. Never a body or a token. */
export function trailApi(instance: typeof api) {
  const started = new WeakMap<object, number>()
  const record = (config: { method?: string } | undefined, outcome: string) => {
    const url = config ? redactUrl(instance.getUri(config)) : '(unknown)'
    const ms = config && started.has(config) ? ` (${Math.round(performance.now() - started.get(config)!)}ms)` : ''
    recordTrail('api', `${(config?.method ?? 'get').toUpperCase()} ${url} → ${outcome}${ms}`)
  }
  instance.interceptors.request.use((config) => {
    started.set(config, performance.now())
    return config
  })
  instance.interceptors.response.use(
    (response) => {
      record(response.config, String(response.status))
      return response
    },
    (error: { config?: { method?: string }; response?: { status?: number }; message?: string }) => {
      record(error?.config, error?.response?.status ? String(error.response.status) : String(error?.message))
      return Promise.reject(error)
    },
  )
}

let installed = false

/** Once, at start-up: clicks, navigations, API calls and uncaught errors. */
export function installBugTrail(router: AnyRouter) {
  if (installed) return
  installed = true
  installClickTrail()
  trailApi(api)
  router.subscribe('onResolved', ({ fromLocation, toLocation }) => {
    recordTrail('nav', `${fromLocation ? redactUrl(fromLocation.href) : '(start)'} → ${redactUrl(toLocation.href)}`)
  })
  window.addEventListener('error', (event) => recordTrail('error', event.message))
  window.addEventListener('unhandledrejection', (event) =>
    recordTrail('error', event.reason instanceof Error ? event.reason.message : String(event.reason)),
  )
}
