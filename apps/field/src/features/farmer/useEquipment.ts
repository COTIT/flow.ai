import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { farmEquipmentGet, farmEquipmentList } from '@genesy/api-client/endpoints/farm-equipment'
import type { EquipmentResource } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import { apiStatus } from '@/lib/apiError'
import { isUuid } from '@/lib/ids'
import { queryKeys } from '@/lib/queryKeys'

/**
 * A machine as the screens read it. Figures the API sends as decimal text (kW, typical hours and
 * days) become numbers for display and the estimate preview; prices stay `Money` (integer minor
 * units), never a float.
 */
export interface EquipmentItem
  extends Omit<EquipmentResource, 'rated_power_kw' | 'typical_hours_per_day' | 'typical_days_per_week'> {
  name: string
  category_name: string
  rated_power_kw: number | null
  typical_hours_per_day: number | null
  typical_days_per_week: number | null
}

/**
 * The catalogue — spec 6.3. Every farm member of the account reads it; names come from the API
 * (name_en / name_sw) for both the item and its category.
 */
export async function fetchEquipment(): Promise<EquipmentResource[]> {
  const workspace = workspaceId()
  return allPages((params) => farmEquipmentList(workspace, { active: true, ...params }))
}

export async function fetchEquipmentItem(id: string): Promise<EquipmentResource | null> {
  // QA #3: a malformed route param reaches the same "not found" as an id matching nothing.
  if (!isUuid(id)) return null
  try {
    return (await farmEquipmentGet(workspaceId(), id)).data
  } catch (error) {
    // 404 is an answer: not listed, or another account's machine.
    if (apiStatus(error) === 404) return null
    throw error
  }
}

const num = (text: string | null) => (text === null ? null : Number(text))

export function localise(row: EquipmentResource, language: string | undefined): EquipmentItem {
  const sw = language === 'sw'
  return {
    ...row,
    name: sw ? row.name_sw : row.name_en,
    category_name: sw ? row.category.name_sw : row.category.name_en,
    rated_power_kw: num(row.rated_power_kw),
    typical_hours_per_day: num(row.typical_hours_per_day),
    typical_days_per_week: num(row.typical_days_per_week),
  }
}

export function useEquipmentList() {
  const { i18n } = useTranslation()
  const query = useQuery({
    queryKey: queryKeys.equipment('mine'),
    queryFn: fetchEquipment,
    staleTime: 5 * 60_000,
  })
  return { ...query, items: (query.data ?? []).map((r) => localise(r, i18n.resolvedLanguage)) }
}

export function useEquipmentItem(id: string) {
  const { i18n } = useTranslation()
  const query = useQuery({
    queryKey: queryKeys.equipmentItem(id),
    queryFn: () => fetchEquipmentItem(id),
    staleTime: 5 * 60_000,
  })
  return {
    ...query,
    item: query.data ? localise(query.data, i18n.resolvedLanguage) : null,
  }
}
