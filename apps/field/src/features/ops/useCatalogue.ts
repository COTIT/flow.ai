import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { EarlierVersionSavedError } from '@/lib/drafts'
import { queryKeys } from '@/lib/queryKeys'
import { recoverInsert } from '@/lib/recoverInsert'
import { supabase } from '@/lib/supabase'

/**
 * Writes to the resource catalogue — spec 7.4, added 5 Oct 2026.
 *
 * `equipment` and `loan_product` are CONFIG tables (no provenance), written
 * directly: `equipment_write` and `loan_product_write` require
 * `app_manages_project`, so RLS decides who may. The rules — offered to rent
 * or buy, a range that runs forwards, a unique code — are the database's,
 * and its messages are shown.
 */

export function useEquipmentCategories() {
  const { i18n } = useTranslation()
  const query = useQuery({
    queryKey: queryKeys.equipmentCategories(),
    queryFn: async () => {
      const { data, error } = await supabase
        .from('equipment_category')
        .select('id, name_en, name_sw')
        .eq('is_active', true)
        .order('name_en')
      if (error) throw new Error(error.message)
      return data ?? []
    },
    staleTime: 5 * 60_000,
  })
  const sw = i18n.resolvedLanguage === 'sw'
  return { ...query, data: query.data?.map((c) => ({ id: c.id, name: sw ? c.name_sw : c.name_en })) }
}

export interface NewEquipment {
  id: string
  project_id: string
  category_id: string
  code: string
  name_en: string
  name_sw: string
  rated_power_kw: number | null
  typical_hours_per_day: number | null
  typical_days_per_week: number | null
  can_rent: boolean
  can_buy: boolean
  indicative_price: number | null
  indicative_rent_per_day: number | null
}

export interface NewLoanProduct {
  id: string
  project_id: string
  code: string
  name_en: string
  name_sw: string
  description_en: string | null
  description_sw: string | null
  indicative_min_amount: number
  indicative_max_amount: number
}

function useCatalogueInsert<T extends { id: string }>(
  table: 'equipment' | 'loan_product',
  queryKey: readonly unknown[],
) {
  const queryClient = useQueryClient()
  const invalidate = () => queryClient.invalidateQueries({ queryKey })

  return useMutation({
    mutationFn: async (row: T) => {
      const { error } = await supabase.from(table).insert(row as never)
      if (error) await recoverInsert(table, row.id, error, { ...row })
    },
    onSuccess: invalidate,
    // The earlier submission saved: the list must hold it.
    onError: async (error) => {
      if (error instanceof EarlierVersionSavedError) await invalidate()
    },
  })
}

export const useCreateEquipment = () =>
  useCatalogueInsert<NewEquipment>('equipment', queryKeys.equipment('mine'))

export const useCreateLoanProduct = () =>
  useCatalogueInsert<NewLoanProduct>('loan_product', queryKeys.loanProducts())
