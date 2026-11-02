import { useEffect, useState } from 'react'
import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  farmPueRequestsCreate,
  farmPueRequestsEstimate,
  farmPueRequestsGet,
  farmPueRequestsList,
  farmPueRequestsTransition,
} from '@genesy/api-client/endpoints/farm-pue'
import type { EnergyEstimatePreview, PueRequestResource } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import type { FarmerAction, RequestStatus, ReviewerAction } from '@/features/ops/transitions'
import { apiStatus } from '@/lib/apiError'
import type { Enums } from '@/lib/enums'
import { isUuid } from '@/lib/ids'
import { queryKeys } from '@/lib/queryKeys'


const num = (text: string | null) => (text === null ? null : Number(text))

/** The API's stored estimate (its database is the only writer), as numbers for display. */
export interface StoredEstimate {
  rated_power_kw: number
  quantity: number
  hours_per_day: number
  days_per_week: number
  est_power_kw: number
  est_kwh_per_day: number
  est_kwh_per_week: number
}

/** A request as the farmer and ops screens read it. */
export interface FarmerRequest {
  id: string
  village_id: string
  farm_id: string | null
  farm_label: string | null
  status: RequestStatus
  acquisition: 'rent' | 'buy'
  quantity: number
  hours_per_day: number | null
  days_per_week: number | null
  purpose: string | null
  submitted_at: string | null
  decided_at: string | null
  decision_note: string | null
  source: Enums['source_type']
  verification: Enums['verification_status']
  confidence: Enums['confidence_level'] | null
  captured_at: string
  captured_by_name: string | null
  applicant: string
  equipment_name: string
  estimate: StoredEstimate | null
}

const toEstimate = (e: EnergyEstimatePreview): StoredEstimate => ({
  rated_power_kw: Number(e.rated_power_kw),
  quantity: e.quantity,
  hours_per_day: Number(e.hours_per_day),
  days_per_week: Number(e.days_per_week),
  est_power_kw: Number(e.est_power_kw),
  est_kwh_per_day: Number(e.est_kwh_per_day),
  est_kwh_per_week: Number(e.est_kwh_per_week),
})

export function toRequest(row: PueRequestResource, language: string | undefined): FarmerRequest {
  return {
    id: row.id,
    village_id: row.village_id,
    farm_id: row.farm_id,
    farm_label: row.farm?.label ?? null,
    status: row.status as RequestStatus,
    acquisition: row.acquisition as 'rent' | 'buy',
    quantity: row.quantity,
    hours_per_day: num(row.hours_per_day),
    days_per_week: num(row.days_per_week),
    purpose: row.purpose,
    submitted_at: row.submitted_at,
    decided_at: row.decided_at,
    decision_note: row.decision_note,
    source: row.provenance.source as Enums['source_type'],
    verification: row.provenance.verification as Enums['verification_status'],
    confidence: row.provenance.confidence as Enums['confidence_level'] | null,
    captured_at: row.provenance.captured_at,
    captured_by_name: row.provenance.captured_by?.name ?? null,
    applicant: row.person.name,
    equipment_name: language === 'sw' ? row.equipment.name_sw : row.equipment.name_en,
    estimate: row.estimate && toEstimate(row.estimate),
  }
}

/** The caller's own requests, newest first: the API scopes a farmer to themselves. */
export function fetchMyRequests(): Promise<PueRequestResource[]> {
  const workspace = workspaceId()
  return allPages((params) => farmPueRequestsList(workspace, params))
}

export async function fetchRequest(id: string): Promise<PueRequestResource | null> {
  // QA #3: a malformed route param reaches the same "not found" as an id matching nothing.
  if (!isUuid(id)) return null
  try {
    return (await farmPueRequestsGet(workspaceId(), id)).data
  } catch (error) {
    // 404 is an answer: someone else's request, or none.
    if (apiStatus(error) === 404) return null
    throw error
  }
}

export interface NewRequest {
  /** Made when the form opens and kept with the draft, so a retry is the same request. */
  id: string
  equipmentId: string
  acquisition: 'rent' | 'buy'
  /** As typed (validated text); hours and days go as decimal text. */
  quantity: string
  hoursPerDay: string
  daysPerWeek: string
  purpose: string
}

/** A farmer files for themselves: no `person_id`. The API stamps provenance and computes the estimate. */
export async function submitRequest(input: NewRequest): Promise<PueRequestResource> {
  const res = await farmPueRequestsCreate(workspaceId(), {
    id: input.id,
    equipment_id: input.equipmentId,
    acquisition: input.acquisition,
    quantity: Number(input.quantity),
    hours_per_day: input.hoursPerDay,
    days_per_week: input.daysPerWeek,
    purpose: input.purpose || null,
    status: 'submitted',
  })
  return res.data
}

/** The figures a preview is asked for: as typed and already valid on the form. */
export type EstimateInput = Pick<NewRequest, 'equipmentId' | 'quantity' | 'hoursPerDay' | 'daysPerWeek'>

/** What the API would store for these figures; `null` for a machine with no rated power. Writes nothing. */
export async function fetchEstimatePreview(input: EstimateInput): Promise<StoredEstimate | null> {
  const res = await farmPueRequestsEstimate(workspaceId(), {
    equipment_id: input.equipmentId,
    quantity: Number(input.quantity),
    hours_per_day: input.hoursPerDay,
    days_per_week: input.daysPerWeek,
  })
  return res.data && toEstimate(res.data)
}

/**
 * The live preview while the farmer types. Asked once typing settles (300 ms); the figures are the
 * query key, so an answer for older figures is never shown for newer ones. `null` asks nothing.
 */
export function useEstimatePreview(input: EstimateInput | null) {
  const key = input ? JSON.stringify(input) : ''
  const [settled, setSettled] = useState(key)
  useEffect(() => {
    const timer = setTimeout(() => setSettled(key), 300)
    return () => clearTimeout(timer)
  }, [key])
  return useQuery({
    queryKey: ['estimate-preview', settled],
    enabled: settled !== '',
    queryFn: () => fetchEstimatePreview(JSON.parse(settled) as EstimateInput),
    // The last figures stay up while the next ones are asked about.
    placeholderData: keepPreviousData,
  })
}

/** Any transition; the API decides whether it is allowed and its refusal is shown as written. */
export async function transitionRequest(
  id: string,
  action: FarmerAction | ReviewerAction,
  note?: string,
): Promise<PueRequestResource> {
  const res = await farmPueRequestsTransition(workspaceId(), id, note ? { action, note } : { action })
  return res.data
}

export function useMyRequests({ enabled = true }: { enabled?: boolean } = {}) {
  const { i18n } = useTranslation()
  const query = useQuery({ queryKey: queryKeys.requests({}), enabled, queryFn: fetchMyRequests })
  return { ...query, requests: (query.data ?? []).map((r) => toRequest(r, i18n.resolvedLanguage)) }
}

export function useRequest(requestId: string) {
  const { i18n } = useTranslation()
  const query = useQuery({ queryKey: queryKeys.request(requestId), queryFn: () => fetchRequest(requestId) })
  return { ...query, request: query.data ? toRequest(query.data, i18n.resolvedLanguage) : null }
}

export function useSubmitRequest() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: submitRequest,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['requests'] }),
  })
}

/** The applicant's own transitions: submit a draft, or withdraw. */
export function useFarmerTransition(requestId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (action: FarmerAction) => transitionRequest(requestId, action),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.request(requestId) })
      await queryClient.invalidateQueries({ queryKey: ['requests'] })
    },
  })
}
