import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { supabase } from '@/lib/supabase'
import { isUuid } from '@/lib/ids'
import { queryKeys } from '@/lib/queryKeys'
import type { Database } from '@/lib/db.types'

type EquipmentRow = Database['public']['Tables']['equipment']['Row']

export interface EquipmentItem extends EquipmentRow {
  name: string
  category_name: string
}

const SELECT =
  'id, project_id, category_id, code, name_en, name_sw, rated_power_kw, typical_hours_per_day, typical_days_per_week, indicative_price, indicative_rent_per_day, can_rent, can_buy, currency, is_active, created_at, updated_at, equipment_category ( name_en, name_sw )'

/**
 * The catalogue — spec 6.3. Scoped by equipment_read to app_projects(), so a
 * farmer sees their own project's list and nothing else.
 *
 * Names come from the database (name_en / name_sw) for both the item and its
 * category: these rows are created at runtime and a repo file cannot translate
 * them.
 */
export async function fetchEquipment(): Promise<Array<Record<string, unknown>>> {
  const { data, error } = await supabase
    .from('equipment')
    .select(SELECT)
    .eq('is_active', true)
    .order('name_en')

  if (error) throw new Error(error.message)
  return (data ?? []) as unknown as Array<Record<string, unknown>>
}

export async function fetchEquipmentItem(id: string): Promise<Record<string, unknown> | null> {
  // QA #3: a malformed route param must reach the same "not found" state as a
  // well-formed id matching nothing, rather than a uuid parse failure.
  if (!isUuid(id)) return null

  const { data, error } = await supabase.from('equipment').select(SELECT).eq('id', id).maybeSingle()
  if (error) throw new Error(error.message)
  // Zero rows is an answer: not listed, or out of this project's scope.
  return (data as unknown as Record<string, unknown>) ?? null
}

function localise(row: Record<string, unknown>, language: string | undefined): EquipmentItem {
  const category = row.equipment_category as { name_en: string; name_sw: string } | null
  const sw = language === 'sw'
  return {
    ...(row as unknown as EquipmentRow),
    name: sw ? (row.name_sw as string) : (row.name_en as string),
    category_name: category ? (sw ? category.name_sw : category.name_en) : '',
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
