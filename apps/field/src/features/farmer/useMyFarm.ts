import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { farmRecordsPersonsHoldings } from '@genesy/api-client/endpoints/farm-records'
import type { Provenance as ApiProvenance } from '@genesy/api-client/model'

import { useSession } from '@/app/session'
import { workspaceId } from '@/app/workspace'
import type { Enums } from '@/lib/enums'
import { queryKeys } from '@/lib/queryKeys'

type Provenance = {
  source: Enums['source_type']
  verification: Enums['verification_status']
  confidence: Enums['confidence_level'] | null
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
 * The farmer's own farms — spec 6.2 — from one holdings read for their person:
 * the farms they manage or their household owns, with plots, cycles and harvests.
 *
 * Only the CURRENT expected figure is shown, so a superseded estimate never
 * appears beside the figure that replaced it.
 */
export async function fetchMyFarms(personId: string, language: string | undefined): Promise<MyFarm[]> {
  const { households, farms } = (await farmRecordsPersonsHoldings(workspaceId(), personId)).data
  const householdLabel = new Map(households.map((h) => [h.id, h.label]))

  return farms
    .map((farm) => ({
      id: farm.id,
      label: farm.label,
      household_label: (farm.household_id && householdLabel.get(farm.household_id)) ?? null,
      ...provenance(farm.provenance),
      plots: farm.plots.map((plot) => ({
        id: plot.id,
        label: plot.label,
        area_ha: num(plot.area_ha),
        ...provenance(plot.provenance),
        cycles: plot.cycles.map((c) => {
          // Harvests come newest first; pick the current one, never compute it.
          const report = c.harvests.find((r) => r.kind === 'expected' && r.is_current) ?? null
          return {
            id: c.id,
            crop_name: (language === 'sw' ? c.crop?.name_sw : c.crop?.name_en) ?? '',
            season_label: c.season_label,
            area_ha: num(c.area_ha),
            tree_count: c.tree_count,
            unit_count: c.unit_count,
            harvest_start: c.harvest_start,
            harvest_end: c.harvest_end,
            expected: report && { id: report.id, quantity_kg: Number(report.quantity_kg), ...provenance(report.provenance) },
            ...provenance(c.provenance),
          }
        }),
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
