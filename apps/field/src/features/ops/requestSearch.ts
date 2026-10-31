import type { RequestStatus } from '@/features/ops/transitions'

export interface RequestSearch {
  status?: RequestStatus
  village?: string
}

const STATUSES: RequestStatus[] = [
  'draft',
  'submitted',
  'under_review',
  'approved',
  'rejected',
  'withdrawn',
]

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/**
 * Filter state lives in the URL (spec 7.2), so it arrives as untrusted text.
 *
 * Anything unrecognised is DROPPED rather than thrown: a stale bookmark or a
 * hand-typed URL should degrade to the unfiltered view, not break the screen.
 * A bad value is also worth dropping rather than forwarding — a non-uuid
 * reaches PostgREST as invalid input for a uuid column and comes back as an
 * error, which is a worse answer than "no filter".
 */
export function validateRequestSearch(search: Record<string, unknown>): RequestSearch {
  const out: RequestSearch = {}

  const status = search.status
  if (typeof status === 'string' && (STATUSES as string[]).includes(status)) {
    out.status = status as RequestStatus
  }

  const village = search.village
  if (typeof village === 'string' && UUID.test(village)) {
    out.village = village
  }

  return out
}
