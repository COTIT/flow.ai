import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  farmEquipmentCategoriesCreate,
  farmEquipmentCategoriesList,
  farmEquipmentCreate,
} from '@genesy/api-client/endpoints/farm-equipment'
import { farmLoansCreate } from '@genesy/api-client/endpoints/farm-loans'
import type { EquipmentCategoryRequest, EquipmentRequest, LoanProductRequest } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import { queryKeys } from '@/lib/queryKeys'

/**
 * Writes to the resource catalogue (equipment and loan listings) — spec 7.4. Ops and admin may
 * (the API refuses others). The rules — offered to rent or buy, a unique code, figure ranges — are the API's, and its messages
 * are shown. Every create carries the draft's id, so a retry replays (200) instead of adding twice.
 */

export function useEquipmentCategories() {
  const { i18n } = useTranslation()
  const query = useQuery({
    queryKey: queryKeys.equipmentCategories(),
    queryFn: () => {
      const workspace = workspaceId()
      return allPages((params) => farmEquipmentCategoriesList(workspace, { active: true, ...params }))
    },
    staleTime: 5 * 60_000,
  })
  const sw = i18n.resolvedLanguage === 'sw'
  return { ...query, data: query.data?.map((c) => ({ id: c.id, code: c.code, name: sw ? c.name_sw : c.name_en })) }
}

export function useCreateCategory() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: EquipmentCategoryRequest) => (await farmEquipmentCategoriesCreate(workspaceId(), body)).data,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.equipmentCategories() }),
  })
}

export function useCreateEquipment() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: EquipmentRequest) => (await farmEquipmentCreate(workspaceId(), body)).data,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.equipment('mine') }),
  })
}

export function useCreateLoanProduct() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: LoanProductRequest) => (await farmLoansCreate(workspaceId(), body)).data,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.loanProducts() }),
  })
}
