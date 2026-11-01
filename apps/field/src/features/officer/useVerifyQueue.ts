import { useMemo } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  farmRecordsCropCyclesList,
  farmRecordsCropCyclesVerify,
  farmRecordsFarmsList,
  farmRecordsFarmsVerify,
  farmRecordsHarvestReportsAll,
  farmRecordsHarvestReportsVerify,
  farmRecordsHouseholdsList,
  farmRecordsHouseholdsVerify,
  farmRecordsPersonsList,
  farmRecordsPersonsVerify,
  farmRecordsPlotsList,
  farmRecordsPlotsVerify,
} from '@genesy/api-client/endpoints/farm-records'
import type { CropName, Provenance as ApiProvenance } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import type { VerifiableTable } from '@/features/officer/personDetail'
import { provenanceOf } from '@/features/officer/useOfficerRecords'
import { localisedName } from '@/lib/names'
import { queryKeys } from '@/lib/queryKeys'
import type { Enums } from '@/lib/enums'


export interface QueueRow {
  table: VerifiableTable
  id: string
  /** What the record is: a name, a label, a cycle's season, or a figure's kg. The hook completes it at render. */
  label: string
  /** A cycle's crop, both names: chosen at render. */
  crop?: CropName
  /** A harvest figure's kind (`expected`/`actual`): worded at render. */
  harvest_kind?: string
  village_id: string
  source: Enums['source_type']
  verification: Enums['verification_status']
  confidence: Enums['confidence_level'] | null
  captured_at: string
  farm_id?: string
  crop_cycle_id?: string
  /**
   * Present only for a household: who registered it. That officer may not
   * verify it — the API refuses, and the screen says so before they try.
   * Convenience; the API decides.
   */
  captured_by?: string | null
}

/**
 * Spec 5.7: records "where `verification in ('unverified','pending')`" — both, because
 * `VerifyButton` renders for either and a queue holding rows nobody can clear would be worse.
 */
export const OUTSTANDING: Enums['verification_status'][] = ['unverified', 'pending']

/** `?verification=` as the API takes it. */
export const OUTSTANDING_FILTER = OUTSTANDING.join(',')

/**
 * The officer's verify queue — spec 5.7: the outstanding records of each kind,
 * `GET …/farm/{kind}?verification=unverified,pending`, every page, harvest figures included
 * (`GET …/farm/harvest-reports`, workspace-wide). The API scopes each list to the caller's role.
 */
export async function fetchVerifyQueue(): Promise<QueueRow[]> {
  const workspace = workspaceId()
  const filter = { verification: OUTSTANDING_FILTER }
  // A failed read is not an empty queue: any one failing rejects the whole queue.
  const [persons, households, farms, plots, cycles, harvests] = await Promise.all([
    allPages((params) => farmRecordsPersonsList(workspace, { ...filter, ...params })),
    allPages((params) => farmRecordsHouseholdsList(workspace, { ...filter, ...params })),
    allPages((params) => farmRecordsFarmsList(workspace, { ...filter, ...params })),
    allPages((params) => farmRecordsPlotsList(workspace, { ...filter, ...params })),
    allPages((params) => farmRecordsCropCyclesList(workspace, { ...filter, ...params })),
    allPages((params) => farmRecordsHarvestReportsAll(workspace, { ...filter, ...params })),
  ])

  const row = (
    table: VerifiableTable,
    record: { id: string; village_id: string; provenance: ApiProvenance },
    label: string,
  ): QueueRow => ({ table, id: record.id, label, village_id: record.village_id, ...provenanceOf(record.provenance) })

  const rows: QueueRow[] = [
    ...persons.map((p) => row('person', p, `${p.given_name} ${p.family_name}`)),
    ...households.map((h) => ({ ...row('household', h, h.label), captured_by: h.provenance.captured_by?.id ?? null })),
    ...farms.map((f) => row('farm', f, f.label)),
    ...plots.map((p) => ({ ...row('plot', p, p.label), farm_id: p.farm_id })),
    ...cycles.map((c) => ({ ...row('crop_cycle', c, c.season_label ?? ''), crop: c.crop })),
    ...harvests.map((h) => ({
      ...row('harvest_report', h, `${h.quantity_kg} kg`),
      crop_cycle_id: h.crop_cycle_id,
      harvest_kind: h.kind,
    })),
  ]

  // Most recently captured first: the queue is worked from the newest
  // registration backwards, which is how an officer's day actually runs.
  return rows.sort((a, b) => b.captured_at.localeCompare(a.captured_at))
}

export function useVerifyQueue() {
  const { t, i18n } = useTranslation()
  const language = i18n.resolvedLanguage
  const query = useQuery({
    queryKey: queryKeys.verifyQueue(),
    queryFn: () => fetchVerifyQueue(),
  })

  // Worded HERE, not in the queryFn: the key has no language, so a label made at fetch would
  // keep the first language (QA #31).
  const data = useMemo(
    () =>
      query.data?.map((row) => {
        const lead = row.crop ? localisedName(row.crop, language) : row.harvest_kind ? t(`harvestKind.${row.harvest_kind}`) : ''
        return lead ? { ...row, label: [lead, row.label].filter(Boolean).join(' · ') } : row
      }),
    [query.data, language, t],
  )
  return { ...query, data }
}

/** `POST /farm/{kind}/{id}/verification`, by table. The API stamps who and when, and refuses a second verify. */
export const VERIFY: Record<VerifiableTable, (workspace: string, id: string) => Promise<unknown>> = {
  person: farmRecordsPersonsVerify,
  household: farmRecordsHouseholdsVerify,
  farm: farmRecordsFarmsVerify,
  plot: farmRecordsPlotsVerify,
  crop_cycle: farmRecordsCropCyclesVerify,
  harvest_report: farmRecordsHarvestReportsVerify,
}

/**
 * Verify one record from the queue. Same routes as the person-detail screen's `useVerify`; only
 * the invalidation differs, because the queue is not scoped to one person.
 */
export function useVerifyFromQueue() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ table, id }: { table: VerifiableTable; id: string }) => {
      await VERIFY[table](workspaceId(), id)
    },
    onSuccess: async (_data, variables) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['verifyQueue'] }),
        queryClient.invalidateQueries({ queryKey: ['officerHome'] }),
        queryClient.invalidateQueries({ queryKey: queryKeys.person(variables.id) }),
        queryClient.invalidateQueries({ queryKey: ['people'] }),
        queryClient.invalidateQueries({ queryKey: ['farm'] }),
        queryClient.invalidateQueries({ queryKey: ['cycle'] }),
        queryClient.invalidateQueries({ queryKey: queryKeys.farms('mine') }),
        queryClient.invalidateQueries({ queryKey: queryKeys.farmerOpportunities() }),
        // Data quality is a Tower figure and moves the moment a record is
        // verified (business-rules §5 invalidation map).
        queryClient.invalidateQueries({
          predicate: (q) => q.queryKey[0] === 'tower',
        }),
      ])
    },
  })
}

export type { VerifiableTable }
