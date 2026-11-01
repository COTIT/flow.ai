import { useQuery } from '@tanstack/react-query'
import { farmMarketDemandsList } from '@genesy/api-client/endpoints/farm-market'
import { farmPueRequestsList } from '@genesy/api-client/endpoints/farm-pue'

import { workspaceId } from '@/app/workspace'
import { queryKeys } from '@/lib/queryKeys'

/**
 * shortcut: "records to verify" is not here. It would be a client sum of six lists' totals, and the
 * Tower rule is that the client does not total; show it again when the API has one count for it
 * (e.g. `GET …/farm/counts` → `outstanding`).
 */
export interface OpsQueues {
  awaitingReview: number
  openDemands: number
}

/**
 * "Requests awaiting review" is submitted plus under_review: both are states an ops user can still
 * act on, and exactly the pair the village energy counts as prospective. The API counts them in one
 * list (`?status=submitted,under_review`); `meta.total` is the figure, never rows counted here.
 */
export async function fetchOpsQueues(): Promise<OpsQueues> {
  const workspace = workspaceId()
  // A failed count rejects: "nothing awaiting review" because a read broke would send ops away from work.
  const [requests, demands] = await Promise.all([
    farmPueRequestsList(workspace, { status: 'submitted,under_review', page_size: 1 }),
    farmMarketDemandsList(workspace, { status: 'open', page_size: 1 }),
  ])
  return { awaitingReview: requests.meta.total, openDemands: demands.meta.total }
}

/** Spec 7.1 — the ops home's queue counts. The API scopes each list to the caller (ops: all villages). */
export function useOpsHome() {
  return useQuery({ queryKey: queryKeys.opsHome(), queryFn: fetchOpsQueues })
}
