import { redirect } from '@tanstack/react-router'

/**
 * Release flag for the resource chain: equipment and loan listings, farmer
 * requests, the ops catalogue and the ops request pipeline (25 Oct 2026).
 *
 * The features work; their catalogue data is not final, so the first
 * production release ships without them. Off unless the build sets
 * `VITE_FEATURE_RESOURCES=on` — a forgotten variable hides pages rather than
 * publishing unfinished listings. Front end only: RLS and the RPCs are
 * unchanged, so this hides screens and is not a security boundary.
 */
export const resourcesEnabled = import.meta.env.VITE_FEATURE_RESOURCES === 'on'

/** A `beforeLoad` that sends a bookmark or drill link to the surface home when the flag is off. */
export function guardResources(to: '/farm' | '/ops', enabled = resourcesEnabled) {
  return () => {
    if (!enabled) throw redirect({ to })
  }
}
