"use client"

import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslations } from "next-intl"

import {
  getWarehousesListQueryKey,
  warehousesCreate,
  warehousesList,
} from "@verzo/api-client/endpoints/warehouses"
import { sessionStore } from "@verzo/api-client/session"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { useCan } from "@/lib/can"
import { allWarehouses, findWarehouse } from "@/lib/warehouses"

const name = "General stock"
const slug = "general-stock"

export function GeneralStock({
  workspaceId,
  onReady,
}: {
  workspaceId: string
  onReady: (warehouseId: string) => void
}) {
  const t = useTranslations()
  const canCreate = useCan("warehouse.create")
  const queryClient = useQueryClient()
  const setup = useMutation({
    mutationFn: async (workspace: string) => {
      const listed = await warehousesList(workspace, allWarehouses)
      const matches = (warehouse: { slug: string }) => warehouse.slug === slug
      if (listed.meta.total > 0)
        return (await findWarehouse(workspace, listed, matches))?.id
      try {
        const result = await warehousesCreate(workspace, { name })
        return result.data.warehouse_id
      } catch (error) {
        if (!fieldErrors(error).slug?.length) throw error
        const refreshed = await warehousesList(workspace, allWarehouses)
        const existing = await findWarehouse(workspace, refreshed, matches)
        if (!existing) throw error
        return existing.id
      }
    },
    onSuccess: (warehouseId, workspace) => {
      if (warehouseId && sessionStore.getState().workspaceId === workspace)
        onReady(warehouseId)
    },
    onSettled: (_result, _error, workspace) =>
      queryClient.invalidateQueries({
        queryKey: getWarehousesListQueryKey(workspace),
      }),
  })

  if (!canCreate) return null
  return (
    <>
      <Button
        type="button"
        variant="outline"
        className="self-start"
        disabled={setup.isPending}
        onClick={() => setup.mutate(workspaceId)}
      >
        {t("warehouses.createGeneral")}
      </Button>
      <ApiError error={setup.error} fallback={t("common.error")} />
    </>
  )
}
