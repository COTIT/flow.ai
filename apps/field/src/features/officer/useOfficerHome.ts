import { useQuery } from '@tanstack/react-query'
import {
  farmRecordsCropCyclesList,
  farmRecordsFarmsList,
  farmRecordsHarvestReportsAll,
  farmRecordsHouseholdsList,
  farmRecordsPersonsList,
  farmRecordsPlotsList,
} from '@genesy/api-client/endpoints/farm-records'

import { useScopeNames } from '@/app/scope'
import { useSession } from '@/app/session'
import { writableVillageIds } from '@/app/membership'
import { workspaceId } from '@/app/workspace'
import { OUTSTANDING_FILTER } from '@/features/officer/useVerifyQueue'
import { queryKeys } from '@/lib/queryKeys'

export interface OfficerVillage {
  villageId: string
  villageName: string
  persons: number
  farms: number
  /** shortcut: equipment requests are not on the API yet, so the count is absent and hidden; fill it when PUE requests move. */
  requests?: number
  unverified: number
}

/**
 * The kinds the verify queue lists. The home's outstanding figure is a link TO that queue, so it
 * counts the same kinds and states (OUTSTANDING_FILTER); a count that disagreed would change the
 * number under the user as they clicked it.
 */
const QUEUE_LISTS = [
  farmRecordsPersonsList,
  farmRecordsHouseholdsList,
  farmRecordsFarmsList,
  farmRecordsPlotsList,
  farmRecordsCropCyclesList,
  farmRecordsHarvestReportsAll,
]

type CountList = (workspace: string, params: { village_id: string; page_size: number; verification?: string }) => Promise<{ meta: { total: number } }>

/** The API's total for one village (one row asked for); never counted from fetched rows. A failed count rejects: it is not zero. */
const count = (read: CountList, villageId: string, verification?: string) =>
  read(workspaceId(), { village_id: villageId, page_size: 1, ...(verification ? { verification } : {}) }).then((r) => r.meta.total)

/**
 * Per-village figures for the officer's home — spec 5.1. The API scopes every count to what this
 * officer may see, so the village ids only decide which buckets to ask for.
 */
export async function fetchOfficerHome(
  villageIds: string[],
): Promise<Array<Omit<OfficerVillage, 'villageName'>>> {
  return Promise.all(
    villageIds.map(async (villageId) => {
      const [persons, farms, ...outstanding] = await Promise.all([
        count(farmRecordsPersonsList, villageId),
        count(farmRecordsFarmsList, villageId),
        ...QUEUE_LISTS.map((read) => count(read, villageId, OUTSTANDING_FILTER)),
      ])
      // A total of exact counts the API returned.
      return { villageId, persons, farms, unverified: outstanding.reduce((sum, n) => sum + n, 0) }
    }),
  )
}

/**
 * Spec 5.1 — assigned villages, their record counts, and what still needs
 * verifying.
 *
 * `writableVillageIds` is the right scope rather than the `village` table:
 * village rows are readable by any signed-in user (they are not personal
 * data), so selecting from them would list villages this officer is not
 * assigned to. The membership is what "assigned" means.
 */
export function useOfficerHome() {
  const { data: session } = useSession()
  const villageIds = writableVillageIds(session?.memberships ?? [])
  const scope = useScopeNames()

  const query = useQuery({
    queryKey: queryKeys.officerHome(villageIds),
    queryFn: () => fetchOfficerHome(villageIds),
    // No villages means nothing to count. Firing the query would return an
    // empty array and read as "loaded and empty", which is the same answer by
    // a slower route.
    enabled: villageIds.length > 0,
  })

  const villages: OfficerVillage[] = (query.data ?? []).map((row) => ({
    ...row,
    villageName: scope.data?.villages[row.villageId] ?? row.villageId,
  }))

  return {
    ...query,
    // Zero assigned villages is a legitimate state (spec 5.1), and with the
    // query disabled it would otherwise report `isLoading` forever.
    isLoading: villageIds.length > 0 && (query.isLoading || scope.isLoading),
    error: query.error ?? scope.error,
    villages,
  }
}
