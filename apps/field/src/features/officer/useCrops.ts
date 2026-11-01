import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { farmCropsList } from '@genesy/api-client/endpoints/farm-crops'
import type { CropResource } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import type { Database } from '@/lib/db.types'

type CropRow = Pick<CropResource, 'id' | 'code' | 'name_en' | 'name_sw'> & {
  measured_by: Database['public']['Enums']['crop_measure']
}

/** Every active crop of the workspace, by English name. The list is paged, so every page is read. */
export async function fetchCrops(): Promise<CropRow[]> {
  const rows: CropResource[] = []
  for (let page = 1, last = 1; page <= last; page++) {
    const res = await farmCropsList(workspaceId(), { page, page_size: 100 })
    rows.push(...res.data)
    last = res.meta.last_page
  }
  return rows
    .filter((c) => c.is_active)
    .sort((a, b) => a.name_en.localeCompare(b.name_en))
    .map(({ id, code, name_en, name_sw, measured_by }) => ({
      id,
      code,
      name_en,
      name_sw,
      measured_by: measured_by as CropRow['measured_by'],
    }))
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
