import { beforeEach, expect, test, vi } from 'vitest'
import { sessionStore } from '@genesy/api-client/session'

const farmCropsList = vi.fn()
vi.mock('@genesy/api-client/endpoints/farm-crops', () => ({
  farmCropsList: (...args: unknown[]) => farmCropsList(...args),
}))

const { fetchCrops } = await import('@/features/officer/useCrops')

const WORKSPACE = '20000000-0000-4000-8000-000000000001'
const crop = (code: string, name_en: string, is_active = true) => ({
  id: `id-${code}`,
  code,
  name_en,
  name_sw: `${name_en} (sw)`,
  measured_by: 'area',
  is_active,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
})
const page = (data: unknown[], current_page: number, last_page: number) => ({
  data,
  meta: { current_page, last_page, per_page: 100, from: 1, to: data.length, total: 0 },
})

beforeEach(() => {
  farmCropsList.mockReset()
  sessionStore.getState().signIn({ access_token: 'tok', token_type: 'Bearer', workspace_ids: [WORKSPACE] })
})

test('reads every page of the workspace crops, keeps the active ones, by English name', async () => {
  farmCropsList
    .mockResolvedValueOnce(page([crop('MZ', 'Maize'), crop('OLD', 'Old', false)], 1, 2))
    .mockResolvedValueOnce(page([crop('CF', 'Coffee')], 2, 2))

  const crops = await fetchCrops()

  expect(farmCropsList).toHaveBeenNthCalledWith(1, WORKSPACE, { page: 1, page_size: 100 })
  expect(farmCropsList).toHaveBeenNthCalledWith(2, WORKSPACE, { page: 2, page_size: 100 })
  expect(crops.map((c) => c.code)).toEqual(['CF', 'MZ'])
  expect(crops[0]).toMatchObject({ id: 'id-CF', name_en: 'Coffee', name_sw: 'Coffee (sw)', measured_by: 'area' })
})
