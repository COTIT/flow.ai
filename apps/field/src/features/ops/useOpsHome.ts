import { useQuery } from '@tanstack/react-query'

import { OUTSTANDING } from '@/features/officer/useVerifyQueue'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { Database } from '@/lib/db.types'

export interface OpsQueues {
  awaitingReview: number
  openDemands: number
  outstandingRecords: number
}

/**
 * "Requests awaiting review" is submitted plus under_review.
 *
 * Both are states an ops user can still act on — `submitted` needs a review
 * started, `under_review` needs a decision — and they are exactly the pair
 * `v_village_energy` counts as prospective. A queue that showed only
 * `submitted` would hide work already in progress.
 */
const AWAITING: Database['public']['Enums']['pue_status'][] = ['submitted', 'under_review']

/** The five tables `app_verify` accepts. */
const VERIFIABLE = ['person', 'farm', 'plot', 'crop_cycle', 'harvest_report'] as const

/**
 * A failed count is not zero. "Nothing awaiting review" because a query broke
 * would send an ops user away from work that is waiting.
 */
function readCount(result: { count: number | null; error: { message: string } | null }): number {
  if (result.error) throw new Error(result.error.message)
  return result.count ?? 0
}

/**
 * Written out per table rather than behind one generic helper. The generated
 * types narrow the query builder to the table it was opened on, so a shared
 * builder has to be cast to one of them and then rejects the others' columns —
 * `status = 'open'` is valid on buyer_demand and not on pue_request.
 */
const HEAD = { count: 'exact', head: true } as const

async function countAwaitingReview(): Promise<number> {
  return readCount(
    await supabase
      .from('pue_request')
      .select('*', HEAD)
      .is('deleted_at', null)
      .in('status', AWAITING),
  )
}

async function countOpenDemands(): Promise<number> {
  return readCount(
    await supabase
      .from('buyer_demand')
      .select('*', HEAD)
      .is('deleted_at', null)
      .eq('status', 'open'),
  )
}

async function countOutstanding(table: (typeof VERIFIABLE)[number]): Promise<number> {
  return readCount(
    await supabase
      .from(table)
      .select('*', HEAD)
      .is('deleted_at', null)
      .in('verification', OUTSTANDING),
  )
}

/**
 * Spec 7.1 — the ops home's queue counts.
 *
 * No project or village filter: ops and admin hold whole-project scope with
 * `village_id` null, and `app_villages()` resolves that to every village in
 * the project, so RLS already answers "mine". Counts are asked of the
 * database with `head: true` — business-rules §11 keeps aggregates there.
 */
export async function fetchOpsQueues(): Promise<OpsQueues> {
  const [awaitingReview, openDemands, ...outstanding] = await Promise.all([
    countAwaitingReview(),
    countOpenDemands(),
    ...VERIFIABLE.map(countOutstanding),
  ])

  return {
    awaitingReview,
    openDemands,
    // Summed from exact counts the database returned, the same definition the
    // officer's verify queue lists, so the two surfaces cannot disagree.
    outstandingRecords: outstanding.reduce((sum, n) => sum + n, 0),
  }
}

export function useOpsHome() {
  return useQuery({ queryKey: queryKeys.opsHome(), queryFn: fetchOpsQueues })
}
