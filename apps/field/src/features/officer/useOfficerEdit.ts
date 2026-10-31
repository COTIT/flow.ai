import { useMutation, useQueryClient } from '@tanstack/react-query'

import {
  buildEditPayload,
  type CropMeasure,
  type EditContext,
  type EditableTable,
  type EditValues,
  validateEdit,
} from '@/features/officer/officerEdit'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { Database, Json } from '@/lib/db.types'

export function useOfficerEdit(measure?: Database['public']['Enums']['crop_measure']) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({
      table,
      id,
      values,
      context,
      measure: selectedMeasure,
    }: {
      table: EditableTable
      id: string
      values: EditValues
      context: EditContext
      measure?: CropMeasure
    }) => {
      const activeMeasure = selectedMeasure ?? measure
      const validationError = validateEdit(table, values, activeMeasure)
      if (validationError) throw new Error(validationError)

      if (table === 'harvest_report') {
        if (!context.cycleId || !context.harvestKind) throw new Error('harvest context is required')
        const { error } = await supabase.rpc('app_supersede_harvest', {
          p_cycle: context.cycleId,
          p_kind: context.harvestKind,
          p_quantity_kg: Number(values.quantity_kg),
          p_source: 'field_verified',
          p_confidence: values.confidence as Database['public']['Enums']['confidence_level'],
          p_reported_for: values.reported_for || undefined,
        })
        if (error) throw new Error(error.message)
        return
      }

      const { error } = await supabase.rpc('app_update_observed_record', {
        p_table: table,
        p_id: id,
        p_payload: buildEditPayload(table, values, activeMeasure) as unknown as Json,
      })
      if (error) throw new Error(error.message)
    },
    onSuccess: async (_data, variables) => {
      const { context } = variables
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.verifyQueue() }),
        queryClient.invalidateQueries({ queryKey: ['officerHome'] }),
        queryClient.invalidateQueries({ queryKey: ['people'] }),
        queryClient.invalidateQueries({ queryKey: ['farm'] }),
        queryClient.invalidateQueries({ queryKey: ['cycle'] }),
        ...(context.farmId ? [queryClient.invalidateQueries({ queryKey: queryKeys.farm(context.farmId) })] : []),
        ...(context.cycleId ? [
          queryClient.invalidateQueries({ queryKey: queryKeys.cycle(context.cycleId) }),
          queryClient.invalidateQueries({ queryKey: queryKeys.harvest(context.cycleId) }),
        ] : []),
        queryClient.invalidateQueries({ queryKey: queryKeys.farms('mine') }),
        queryClient.invalidateQueries({ queryKey: queryKeys.farmerOpportunities() }),
        queryClient.invalidateQueries({ queryKey: ['tower'] }),
        ...(context.personId ? [queryClient.invalidateQueries({ queryKey: queryKeys.person(context.personId) })] : []),
        ...(context.villageId
          ? [queryClient.invalidateQueries({ queryKey: queryKeys.people(context.villageId) })]
          : []),
      ])
    },
  })
}
