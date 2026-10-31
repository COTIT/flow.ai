import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { supabase } from '@/lib/supabase'
import { isUuid } from '@/lib/ids'
import { queryKeys, isTowerQueryForVillage } from '@/lib/queryKeys'
import type { PersonDetail, VerifiableTable } from '@/features/officer/personDetail'

/**
 * Person detail as a graph read.
 *
 * Nested select() rather than chained round trips: one request, and RLS applies
 * all the way down — an officer outside the village gets nothing at every
 * level rather than a partial tree.
 *
 * Farms reach the person two ways: they manage it, or their household owns it.
 * Both are resolved before the graph read so the nesting stays one query.
 */
export async function fetchPersonDetail(personId: string): Promise<PersonDetail | null> {
  // QA #3: a malformed route param must reach the same "not found" state as a
  // well-formed id matching nothing, rather than a uuid parse failure.
  if (!isUuid(personId)) return null

  const personResult = await supabase
    .from('person')
    .select(
      'id, given_name, family_name, phone, village_id, source, verification, confidence, captured_at, captured_by, verified_by, verified_at',
    )
    .eq('id', personId)
    .maybeSingle()

  if (personResult.error) throw new Error(personResult.error.message)
  // Zero rows is an answer: RLS says this person is not visible.
  if (!personResult.data) return null

  const membershipResult = await supabase
    .from('household_member')
    .select('household_id')
    .eq('person_id', personId)
  if (membershipResult.error) throw new Error(membershipResult.error.message)
  const householdIds = (membershipResult.data ?? []).map((r) => r.household_id)

  const managedResult = await supabase.from('farm_manager').select('farm_id').eq('person_id', personId)
  if (managedResult.error) throw new Error(managedResult.error.message)
  const managedFarmIds = (managedResult.data ?? []).map((r) => r.farm_id)

  const [householdsResult, farmsResult] = await Promise.all([
    householdIds.length
      ? supabase
          .from('household')
          .select(
            'id, label, source, verification, confidence, captured_at, captured_by, verified_by, verified_at, household_member ( person ( id, given_name, family_name ) )',
          )
          .in('id', householdIds)
          .is('deleted_at', null)
      : Promise.resolve({ data: [], error: null }),
    supabase
      .from('farm')
      .select(
        // Every embed names its foreign key. village_id is denormalised down
        // farm -> plot -> crop_cycle -> harvest_report and held true by
        // composite FKs, so each of those pairs has TWO relationships and
        // PostgREST refuses to guess: "more than one relationship was found".
        // The simple id FK is the one to traverse.
        `id, label, latitude, longitude, source, verification, confidence, captured_at, captured_by, verified_by, verified_at,
         plot!plot_farm_id_fkey ( id, label, area_ha, latitude, longitude, source, verification, confidence, captured_at, captured_by, verified_by, verified_at,
           crop_cycle!crop_cycle_plot_id_fkey ( id, crop_id, season_label, area_ha, tree_count, unit_count, planted_on, harvest_start, harvest_end, status,
             source, verification, confidence, captured_at, captured_by, verified_by, verified_at,
             crop ( name_en, name_sw ),
             harvest_report!harvest_report_crop_cycle_id_fkey ( id, kind, quantity_kg, is_current, reported_for, source, verification, confidence, captured_at, captured_by, verified_by, verified_at )
           )
         )`,
      )
      .or(
        [
          managedFarmIds.length ? `id.in.(${managedFarmIds.join(',')})` : null,
          householdIds.length ? `household_id.in.(${householdIds.join(',')})` : null,
        ]
          .filter(Boolean)
          .join(',') || 'id.eq.00000000-0000-0000-0000-000000000000',
      )
      .is('deleted_at', null),
  ])

  if (householdsResult.error) throw new Error(householdsResult.error.message)
  if (farmsResult.error) throw new Error(farmsResult.error.message)

  return {
    person: personResult.data as PersonDetail['person'],
    households: (householdsResult.data ?? []).map((h) => {
      const raw = h as unknown as Record<string, unknown>
      const links = (raw.household_member ?? []) as Array<{
        person: { id: string; given_name: string; family_name: string } | null
      }>
      return {
        ...(h as unknown as PersonDetail['households'][number]),
        members: links.map((l) => l.person).filter((p): p is NonNullable<typeof p> => Boolean(p)),
      }
    }),
    farms: (farmsResult.data ?? []).map((f) => {
      const raw = f as unknown as Record<string, unknown>
      const plots = (raw.plot ?? []) as Array<Record<string, unknown>>
      return {
        ...(f as unknown as PersonDetail['farms'][number]),
        plots: plots.map((p) => ({
          ...(p as unknown as PersonDetail['farms'][number]['plots'][number]),
          cycles: ((p.crop_cycle ?? []) as Array<Record<string, unknown>>).map((c) => ({
            ...(c as unknown as PersonDetail['farms'][number]['plots'][number]['cycles'][number]),
            harvests: (c.harvest_report ?? []) as PersonDetail['farms'][number]['plots'][number]['cycles'][number]['harvests'],
          })),
        })),
      }
    }),
  }
}

export function usePersonDetail(personId: string) {
  const { i18n } = useTranslation()
  const query = useQuery({
    queryKey: queryKeys.person(personId),
    queryFn: () => fetchPersonDetail(personId),
  })

  // Crop names are translated in the database, so the label depends on locale.
  const language = i18n.resolvedLanguage
  const data = query.data
    ? {
        ...query.data,
        farms: query.data.farms.map((farm) => ({
          ...farm,
          plots: farm.plots.map((plot) => ({
            ...plot,
            cycles: plot.cycles.map((cycle) => {
              const crop = (cycle as unknown as { crop?: { name_en: string; name_sw: string } }).crop
              return {
                ...cycle,
                crop_name: crop ? (language === 'sw' ? crop.name_sw : crop.name_en) : cycle.crop_id,
              }
            }),
          })),
        })),
      }
    : query.data

  return { ...query, data }
}

/**
 * Verification goes through app_verify and nowhere else.
 *
 * One entry point means verification can never be set without a verifier:
 * the function stamps verified_by = auth.uid() and verified_at itself, and
 * refuses a table that is not verifiable.
 */
export function useVerify(personId: string, villageId: string | undefined) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ table, id }: { table: VerifiableTable; id: string }) => {
      const { error } = await supabase.rpc('app_verify', { p_table: table, p_id: id })
      if (error) throw new Error(error.message)
    },
    // Invalidate the record key plus the farmer-facing keys and the Tower's
    // data-quality view, so the badge flips on every surface without a manual
    // refresh.
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.person(personId) })
      if (villageId) {
      await queryClient.invalidateQueries({ queryKey: queryKeys.people(villageId) })
      await queryClient.invalidateQueries({ queryKey: queryKeys.farms(villageId) })
      await queryClient.invalidateQueries({ queryKey: queryKeys.farms('mine') })
      await queryClient.invalidateQueries({ queryKey: queryKeys.farmerOpportunities() })
      await queryClient.invalidateQueries({ predicate: isTowerQueryForVillage(villageId) })
      }
    },
  })
}
