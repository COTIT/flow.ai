import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { farmCropsList } from '@genesy/api-client/endpoints/farm-crops'
import {
  farmRecordsCropCyclesList,
  farmRecordsFarmsList,
  farmRecordsHarvestReportsList,
  farmRecordsHouseholdsList,
  farmRecordsPlotsList,
} from '@genesy/api-client/endpoints/farm-records'
import type { Provenance as ApiProvenance } from '@genesy/api-client/model'

import { useSession } from '@/app/session'
import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import type { Database } from '@/lib/db.types'
import { queryKeys } from '@/lib/queryKeys'

type Provenance = {
  source: Database['public']['Enums']['source_type']
  verification: Database['public']['Enums']['verification_status']
  confidence: Database['public']['Enums']['confidence_level'] | null
  captured_at: string
}

export interface MyFarm extends Provenance {
  id: string
  label: string
  /** The owning household, when the farm belongs to the farmer's household. */
  household_label: string | null
  plots: Array<
    Provenance & {
      id: string
      label: string
      area_ha: number | null
      cycles: Array<
        Provenance & {
          id: string
          crop_name: string
          season_label: string | null
          area_ha: number | null
          tree_count: number | null
          unit_count: number | null
          harvest_start: string | null
          harvest_end: string | null
          expected: (Provenance & { id: string; quantity_kg: number }) | null
        }
      >
    }
  >
}

const provenance = ({ source, verification, confidence, captured_at }: ApiProvenance): Provenance => ({
  source: source as Provenance['source'],
  verification: verification as Provenance['verification'],
  confidence: confidence as Provenance['confidence'],
  captured_at,
})

// Decimal text from the API, as a number for the display formatters only.
const num = (text: string | null) => (text === null ? null : Number(text))

/**
 * The farmer's own farms — spec 6.2 — joined on the client from the farm
 * record lists, for display only.
 *
 * The API already scopes a farmer's reads to what hangs off their person.
 * The person filter here is for mixed roles: someone who is a farmer in one
 * village and an officer in another also gets that village's records.
 *
 * Only CURRENT expected figures are shown (`?current=true`), so a superseded
 * estimate never appears beside the figure that replaced it.
 */
export async function fetchMyFarms(personId: string, language: string | undefined): Promise<MyFarm[]> {
  const ws = workspaceId()
  const [households, farms, plots, cycles, crops] = await Promise.all([
    allPages((params) => farmRecordsHouseholdsList(ws, params)),
    allPages((params) => farmRecordsFarmsList(ws, params)),
    allPages((params) => farmRecordsPlotsList(ws, params)),
    allPages((params) => farmRecordsCropCyclesList(ws, params)),
    allPages((params) => farmCropsList(ws, params)),
  ])

  const myHouseholds = new Map(
    households.filter((h) => h.members.some((m) => m.person_id === personId)).map((h) => [h.id, h.label]),
  )
  const myFarms = farms.filter(
    (f) => f.managers.some((m) => m.person_id === personId) || (f.household_id !== null && myHouseholds.has(f.household_id)),
  )
  const farmIds = new Set(myFarms.map((f) => f.id))
  const myPlots = plots.filter((p) => farmIds.has(p.farm_id))
  const plotIds = new Set(myPlots.map((p) => p.id))
  const myCycles = cycles.filter((c) => plotIds.has(c.plot_id))
  const cropName = new Map(crops.map((c) => [c.id, language === 'sw' ? c.name_sw : c.name_en]))

  // shortcut: one harvest-reports read per cycle (N+1); move to a farm-tree endpoint if farmers grow past a handful of cycles.
  const expected = new Map(
    await Promise.all(
      myCycles.map(async (c) => {
        const reports = (await farmRecordsHarvestReportsList(ws, c.id, { current: true, page_size: 100 })).data
        const report = reports.find((r) => r.kind === 'expected') ?? null
        return [
          c.id,
          report && { id: report.id, quantity_kg: Number(report.quantity_kg), ...provenance(report.provenance) },
        ] as const
      }),
    ),
  )

  return myFarms
    .map((farm) => ({
      id: farm.id,
      label: farm.label,
      household_label: (farm.household_id && myHouseholds.get(farm.household_id)) ?? null,
      ...provenance(farm.provenance),
      plots: myPlots
        .filter((p) => p.farm_id === farm.id)
        .map((plot) => ({
          id: plot.id,
          label: plot.label,
          area_ha: num(plot.area_ha),
          ...provenance(plot.provenance),
          cycles: myCycles
            .filter((c) => c.plot_id === plot.id)
            .map((c) => ({
              id: c.id,
              crop_name: cropName.get(c.crop_id) ?? '',
              season_label: c.season_label,
              area_ha: num(c.area_ha),
              tree_count: c.tree_count,
              unit_count: c.unit_count,
              harvest_start: c.harvest_start,
              harvest_end: c.harvest_end,
              expected: expected.get(c.id) ?? null,
              ...provenance(c.provenance),
            })),
        })),
    }))
    .sort((a, b) => a.label.localeCompare(b.label))
}

export function useMyFarm() {
  const { i18n } = useTranslation()
  const language = i18n.resolvedLanguage
  const personId = useSession().data?.appUser.person_id ?? null

  const query = useQuery({
    queryKey: [...queryKeys.farms('mine'), personId, language],
    queryFn: () => fetchMyFarms(personId as string, language),
    // No person: the account is not linked to a registered farmer, so there is no farm to read.
    enabled: personId !== null,
  })

  return { ...query, farms: query.data ?? [] }
}
