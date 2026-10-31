/**
 * Screens already moved from Supabase to the Genesy API (the port, steps 2+). Every other screen
 * under /farm, /officer and /ops renders "not available yet" instead of mounting, so nothing
 * calls Supabase once the account lives in Genesy. Exact paths or `prefix/*`.
 * shortcut: a transitional allow-list; deleted when the last screen is ported (port step 8).
 */
export const PORTED: readonly string[] = [
  '/officer/register',
  '/officer/redeem',
  '/ops/surveys/redemptions',
  '/officer/cycles/*',
  '/farm',
  '/farm/my-farm',
  '/farm/surveys',
  '/farm/surveys/*',
]

export function isPorted(pathname: string, ported: readonly string[] = PORTED): boolean {
  const path = pathname.replace(/\/+$/, '') || '/'
  return ported.some((p) => (p.endsWith('/*') ? path.startsWith(p.slice(0, -1)) : path === p))
}

/** The bug-report button writes to Supabase storage until the API has bug reports (port step 7). */
export const BUG_REPORTS_PORTED = false
