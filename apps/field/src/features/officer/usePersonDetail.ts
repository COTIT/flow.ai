import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { farmRecordsPersonsGet, farmRecordsPersonsHoldings } from '@genesy/api-client/endpoints/farm-records'
import type { Provenance as ApiProvenance } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { apiStatus } from '@/lib/apiError'
import { isUuid } from '@/lib/ids'
import { localisedName } from '@/lib/names'
import { queryKeys, isTowerQueryForVillage } from '@/lib/queryKeys'
import type { PersonDetail, Provenance, VerifiableTable } from '@/features/officer/personDetail'
import { VERIFY } from '@/features/officer/useVerifyQueue'

type Cycle = PersonDetail['farms'][number]['plots'][number]['cycles'][number]
type Harvest = Cycle['harvests'][number]

/** The API's provenance block, flat, with the actors as ids (the four-eyes check compares ids). */
function provenance(p: ApiProvenance): Provenance {
  return {
    source: p.source as Provenance['source'],
    verification: p.verification as Provenance['verification'],
    confidence: p.confidence as Provenance['confidence'],
    captured_at: p.captured_at,
    captured_by: p.captured_by?.id ?? null,
    verified_by: p.verified_by?.id ?? null,
    verified_at: p.verified_at,
  }
}

/** Decimal text from the API as a number, for display only (GPS, hectares, kilograms). */
const shown = (value: string | null) => (value === null ? null : Number(value))

/**
 * Person detail: `GET …/persons/{person}` and `GET …/persons/{person}/holdings`, which brings the
 * households (with their people's names), the farms, plots, cycles with crop names, and harvest figures.
 * The API decides what this officer may see at every level.
 */
export async function fetchPersonDetail(personId: string): Promise<PersonDetail | null> {
  // QA #3: a malformed route param must reach the same "not found" state as a
  // well-formed id matching nothing, rather than a uuid parse failure.
  if (!isUuid(personId)) return null

  const workspace = workspaceId()
  let person
  try {
    person = (await farmRecordsPersonsGet(workspace, personId)).data
  } catch (error) {
    // Another village's person and a missing one are the same answer: not found.
    if (apiStatus(error) === 404) return null
    throw error
  }
  const holdings = (await farmRecordsPersonsHoldings(workspace, personId)).data

  return {
    person: {
      id: person.id,
      given_name: person.given_name,
      family_name: person.family_name,
      phone: person.phone,
      village_id: person.village_id,
      ...provenance(person.provenance),
    },
    households: holdings.households.map((h) => ({
      id: h.id,
      label: h.label,
      ...provenance(h.provenance),
      members: h.people.map((m) => ({ id: m.person_id, given_name: m.given_name, family_name: m.family_name })),
    })),
    farms: holdings.farms.map((farm) => ({
      id: farm.id,
      label: farm.label,
      latitude: shown(farm.latitude),
      longitude: shown(farm.longitude),
      ...provenance(farm.provenance),
      plots: farm.plots.map((plot) => ({
        id: plot.id,
        label: plot.label,
        area_ha: shown(plot.area_ha),
        latitude: shown(plot.latitude),
        longitude: shown(plot.longitude),
        ...provenance(plot.provenance),
        cycles: plot.cycles.map((c) => ({
          id: c.id,
          crop_id: c.crop_id,
          // Both names cached; the hook picks the language at render (QA #31).
          crop: c.crop ? { name_en: c.crop.name_en, name_sw: c.crop.name_sw } : null,
          crop_name: '',
          season_label: c.season_label,
          area_ha: shown(c.area_ha),
          tree_count: c.tree_count,
          unit_count: c.unit_count,
          planted_on: c.planted_on,
          harvest_start: c.harvest_start,
          harvest_end: c.harvest_end,
          status: c.status as Cycle['status'],
          ...provenance(c.provenance),
          harvests: c.harvests.map((h) => ({
            id: h.id,
            kind: h.kind as Harvest['kind'],
            // Display only: a weight, never money, and never summed here.
            quantity_kg: Number(h.quantity_kg),
            is_current: h.is_current,
            reported_for: h.reported_for,
            ...provenance(h.provenance),
          })),
        })),
      })),
    })),
  }
}

export function usePersonDetail(personId: string) {
  const { i18n } = useTranslation()
  const query = useQuery({
    queryKey: queryKeys.person(personId),
    queryFn: () => fetchPersonDetail(personId),
  })

  // Crop names come in both languages, so the label depends on locale.
  const language = i18n.resolvedLanguage
  const data = query.data
    ? {
        ...query.data,
        farms: query.data.farms.map((farm) => ({
          ...farm,
          plots: farm.plots.map((plot) => ({
            ...plot,
            cycles: plot.cycles.map((cycle) => ({
              ...cycle,
              crop_name: localisedName(cycle.crop, language) || cycle.crop_id,
            })),
          })),
        })),
      }
    : query.data

  return { ...query, data }
}

/**
 * `POST …/farm/{kind}/{id}/verification`: the API stamps who and when, and refuses a second verify
 * or a household verified by the officer who registered it.
 */
export function useVerify(personId: string, villageId: string | undefined) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ table, id }: { table: VerifiableTable; id: string }) => {
      await VERIFY[table](workspaceId(), id)
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
