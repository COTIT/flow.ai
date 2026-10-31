import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { supabase } from '@/lib/supabase'
import type { Database } from '@/lib/db.types'

type CropRow = Pick<
  Database['public']['Tables']['crop']['Row'],
  'id' | 'code' | 'name_en' | 'name_sw' | 'measured_by'
>

export async function fetchCrops(): Promise<CropRow[]> {
  const { data, error } = await supabase
    .from('crop')
    .select('id, code, name_en, name_sw, measured_by')
    .eq('is_active', true)
    .order('name_en')

  if (error) throw new Error(error.message)
  return data ?? []
}

/**
 * Crop names are translated IN THE DATABASE (name_en / name_sw), not in the
 * i18n JSON: these rows are created at runtime and a repo file cannot
 * translate them.
 */
export function useCrops() {
  const { i18n } = useTranslation()
  const query = useQuery({ queryKey: ['crops'], queryFn: fetchCrops, staleTime: 5 * 60_000 })

  const localised = (query.data ?? []).map((c) => ({
    ...c,
    name: i18n.resolvedLanguage === 'sw' ? c.name_sw : c.name_en,
  }))

  return { ...query, crops: localised }
}
