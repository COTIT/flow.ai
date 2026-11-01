import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { farmPueRequestsList, farmPueVillageEnergy } from '@genesy/api-client/endpoints/farm-pue'
import type { PueRequestResource } from '@genesy/api-client/model'

import { useScopeNames } from '@/app/scope'
import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import {
  fetchRequest,
  toRequest,
  transitionRequest,
  type FarmerRequest,
} from '@/features/farmer/useRequests'
import type { RequestSearch } from '@/features/ops/requestSearch'
import type { ReviewerAction } from '@/features/ops/transitions'
import { queryKeys } from '@/lib/queryKeys'

export interface OpsRequest extends FarmerRequest {
  village_name: string
}

/** The pipeline — spec 7.2. Filters go to the API, never applied to fetched rows. */
export function fetchOpsRequests(search: RequestSearch): Promise<PueRequestResource[]> {
  const workspace = workspaceId()
  const filters = {
    ...(search.status ? { status: search.status } : {}),
    ...(search.village ? { village_id: search.village } : {}),
  }
  return allPages((params) => farmPueRequestsList(workspace, { ...filters, ...params }))
}

/** The API's village energy (capacity, peaks, headroom), as numbers for display. */
export async function fetchVillageEnergy(villageId: string) {
  const e = (await farmPueVillageEnergy(workspaceId(), villageId)).data
  const num = (text: string | null) => (text === null ? null : Number(text))
  return {
    capacity_kw: num(e.capacity_kw),
    capacity_basis: e.capacity_basis,
    simultaneity_factor: num(e.simultaneity_factor),
    prospective_peak_kw: num(e.prospective_peak_kw),
    approved_peak_kw: num(e.approved_peak_kw),
    headroom_kw: num(e.headroom_kw),
  }
}

function useShape() {
  const { i18n } = useTranslation()
  const scope = useScopeNames()
  // Village names joined on the client, display only.
  return (row: PueRequestResource): OpsRequest => ({
    ...toRequest(row, i18n.resolvedLanguage),
    village_name: scope.data?.villages[row.village_id] ?? '',
  })
}

export function useOpsRequests(search: RequestSearch) {
  const shape = useShape()
  const query = useQuery({
    queryKey: queryKeys.requests({ villageId: search.village, status: search.status }),
    queryFn: () => fetchOpsRequests(search),
  })
  return { ...query, requests: (query.data ?? []).map(shape) }
}

export function useOpsRequest(requestId: string) {
  const shape = useShape()
  const query = useQuery({ queryKey: queryKeys.request(requestId), queryFn: () => fetchRequest(requestId) })
  return { ...query, request: query.data ? shape(query.data) : null }
}

export function useVillageEnergy(villageId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.tower.energy(villageId ?? ''),
    enabled: Boolean(villageId),
    queryFn: () => fetchVillageEnergy(villageId!),
  })
}

/** start_review, approve or reject; the API stamps who decided and when, and refuses what is not allowed. */
export function useReviewAction(requestId: string, villageId: string | undefined) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ action, note }: { action: ReviewerAction; note?: string }) =>
      transitionRequest(requestId, action, note),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.request(requestId) })
      await queryClient.invalidateQueries({ queryKey: ['requests'] })
      if (villageId) await queryClient.invalidateQueries({ queryKey: queryKeys.tower.energy(villageId) })
    },
  })
}
