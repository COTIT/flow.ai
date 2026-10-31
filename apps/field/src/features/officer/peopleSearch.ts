import type { Database } from '@/lib/db.types'

export type VerificationStatus = Database['public']['Enums']['verification_status']

export interface PeopleSearch {
  /** Free-text name search. */
  q?: string
  verification?: VerificationStatus
}

const VERIFICATIONS: VerificationStatus[] = ['unverified', 'pending', 'verified', 'disputed']

/**
 * A name search long enough to be a real name and short enough not to be an
 * attack on the URL bar. Nothing in the schema has a name near this length.
 */
const MAX_Q = 100

/**
 * Filter state lives in the URL (spec §10), so it arrives as untrusted text.
 *
 * Anything unrecognised is DROPPED rather than thrown, exactly as
 * `validateRequestSearch` does: a stale bookmark or a hand-typed URL should
 * degrade to the unfiltered view rather than break the screen. Forwarding a
 * bad value is worse than dropping it — an unknown enum member reaches
 * PostgREST as `invalid input value for enum verification_status`, which is a
 * poorer answer than "no filter".
 */
export function validatePeopleSearch(search: Record<string, unknown>): PeopleSearch {
  const out: PeopleSearch = {}

  const q = search.q
  if (typeof q === 'string') {
    const trimmed = q.trim().slice(0, MAX_Q)
    // A whitespace-only search is not a search.
    if (trimmed.length > 0) out.q = trimmed
  }

  const verification = search.verification
  if (
    typeof verification === 'string' &&
    (VERIFICATIONS as string[]).includes(verification)
  ) {
    out.verification = verification as VerificationStatus
  }

  return out
}

export { VERIFICATIONS }
