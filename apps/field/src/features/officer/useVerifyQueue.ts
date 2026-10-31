import { useMemo } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import type { VerifiableTable } from '@/features/officer/personDetail'
import { localisedName, type LocalisedNames } from '@/lib/names'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { Database } from '@/lib/db.types'

type Enums = Database['public']['Enums']

export interface QueueRow {
  table: VerifiableTable
  id: string
  /**
   * What the record is. For a crop cycle this is the season alone and `crop`
   * carries the names — the two are joined at render, in whichever language is
   * showing then (QA #31).
   */
  label: string
  /** Present only for a crop cycle. Both names; neither chosen yet. */
  crop?: LocalisedNames | null
  village_id: string
  source: Enums['source_type']
  verification: Enums['verification_status']
  confidence: Enums['confidence_level'] | null
  captured_at: string
  farm_id?: string
  crop_cycle_id?: string
  /**
   * Present only for a household: who registered it. That officer may not
   * verify it (20260929090002_household_four_eyes) — app_verify refuses, and
   * the screen says so before they try. Convenience; the database decides.
   */
  captured_by?: string | null
}

/**
 * Spec 5.7: records "where `verification in ('unverified','pending')`".
 *
 * Both, not just unverified — the spec names both, and `VerifyButton` already
 * renders for either and hides only on `verified`, so the queue inherits the
 * behaviour the person-detail screen has always had rather than inventing a
 * new rule. Business-rules §5's "MVP only moves unverified → verified"
 * forbids building a DISPUTE flow; it does not make a pending record
 * unactionable, and a queue holding rows nobody can clear would be worse.
 */
export const OUTSTANDING: Enums['verification_status'][] = ['unverified', 'pending']

/** Every table below is one `app_verify` accepts. Anything else raises (business-rules §5). */
const PROVENANCE = 'village_id, source, verification, confidence, captured_at'

/**
 * The officer's verify queue — spec 5.7.
 *
 * Six reads rather than one, because these are six tables with no common
 * parent. No village filter is applied: every one of them is scoped by its own
 * `*_read_staff` policy to `app_villages()`, so "outstanding in my villages" is
 * already the whole query.
 */
export async function fetchVerifyQueue(): Promise<QueueRow[]> {
  const [persons, households, farms, plots, cycles, harvests] = await Promise.all([
    supabase
      .from('person')
      .select(`id, given_name, family_name, ${PROVENANCE}`)
      .in('verification', OUTSTANDING)
      .is('deleted_at', null),
    supabase
      .from('household')
      .select(`id, label, captured_by, ${PROVENANCE}`)
      .in('verification', OUTSTANDING)
      .is('deleted_at', null),
    supabase
      .from('farm')
      .select(`id, label, ${PROVENANCE}`)
      .in('verification', OUTSTANDING)
      .is('deleted_at', null),
    supabase
      .from('plot')
      .select(`id, farm_id, label, ${PROVENANCE}`)
      .in('verification', OUTSTANDING)
      .is('deleted_at', null),
    supabase
      .from('crop_cycle')
      .select(`id, season_label, ${PROVENANCE}, crop ( name_en, name_sw )`)
      .in('verification', OUTSTANDING)
      .is('deleted_at', null),
    supabase
      .from('harvest_report')
      .select(`id, crop_cycle_id, kind, quantity_kg, is_current, ${PROVENANCE}`)
      .in('verification', OUTSTANDING)
      .is('deleted_at', null),
  ])

  for (const r of [persons, households, farms, plots, cycles, harvests]) {
    // A failed read is not an empty queue. Showing "nothing to verify" because
    // one of six queries broke would tell an officer their work is done.
    if (r.error) throw new Error(r.error.message)
  }

  const rows: QueueRow[] = []
  const push = (
    table: VerifiableTable,
    raw: Record<string, unknown>,
    label: string,
    crop?: LocalisedNames | null,
  ) =>
    rows.push({
      table,
      id: raw.id as string,
      label,
      crop,
      village_id: raw.village_id as string,
      source: raw.source as QueueRow['source'],
      verification: raw.verification as QueueRow['verification'],
      confidence: (raw.confidence ?? null) as QueueRow['confidence'],
      captured_at: raw.captured_at as string,
      farm_id: raw.farm_id as string | undefined,
      crop_cycle_id: raw.crop_cycle_id as string | undefined,
      ...(table === 'household' ? { captured_by: raw.captured_by as string | null } : {}),
    })

  for (const p of persons.data ?? []) {
    push('person', p, `${p.given_name} ${p.family_name}`)
  }
  for (const h of households.data ?? []) push('household', h, h.label)
  for (const f of farms.data ?? []) push('farm', f, f.label)
  for (const p of plots.data ?? []) push('plot', p, p.label)
  for (const c of cycles.data ?? []) {
    const raw = c as unknown as Record<string, unknown>
    // The crop name is attached, not resolved: the label is completed at
    // render, in the language showing then — QA #31.
    const crop = raw.crop as LocalisedNames | null
    const season = (raw.season_label as string) ?? ''
    push('crop_cycle', raw, season, crop)
  }
  for (const h of harvests.data ?? []) {
    const raw = h as unknown as Record<string, unknown>
    push('harvest_report', raw, `${raw.kind as string} ${raw.quantity_kg as number} kg`)
  }

  // Most recently captured first: the queue is worked from the newest
  // registration backwards, which is how an officer's day actually runs.
  return rows.sort((a, b) => b.captured_at.localeCompare(a.captured_at))
}

export function useVerifyQueue() {
  const { i18n } = useTranslation()
  const language = i18n.resolvedLanguage

  const query = useQuery({
    queryKey: queryKeys.verifyQueue(),
    queryFn: () => fetchVerifyQueue(),
  })

  /**
   * The label is completed HERE, not in the queryFn.
   *
   * The key is about the officer's villages, so a crop name chosen at fetch
   * time would be served in whichever locale resolved first (QA #31). Doing it
   * at render also means one cache entry rather than one per language, and a
   * language switch that shows immediately.
   */
  const data = useMemo(
    () =>
      query.data?.map((row) =>
        row.crop
          ? { ...row, label: [localisedName(row.crop, language), row.label].filter(Boolean).join(' · ') }
          : row,
      ),
    [query.data, language],
  )

  return { ...query, data }
}

/**
 * Verify one record from the queue.
 *
 * Same single entry point as the person-detail screen's `useVerify`:
 * `app_verify` stamps `verified_by` and `verified_at` itself and refuses a
 * table that is not verifiable. Only the invalidation differs, because the
 * queue is not scoped to one person.
 */
export function useVerifyFromQueue() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ table, id }: { table: VerifiableTable; id: string }) => {
      const { error } = await supabase.rpc('app_verify', { p_table: table, p_id: id })
      if (error) throw new Error(error.message)
    },
    onSuccess: async (_data, variables) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['verifyQueue'] }),
        queryClient.invalidateQueries({ queryKey: ['officerHome'] }),
        queryClient.invalidateQueries({ queryKey: queryKeys.person(variables.id) }),
        queryClient.invalidateQueries({ queryKey: ['people'] }),
        queryClient.invalidateQueries({ queryKey: ['farm'] }),
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
