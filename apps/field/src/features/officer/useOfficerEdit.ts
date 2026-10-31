import { useMutation, useQueryClient } from '@tanstack/react-query'
import {
  farmRecordsCropCyclesCorrect,
  farmRecordsHarvestReportsRevise,
} from '@genesy/api-client/endpoints/farm-records'

import { workspaceId } from '@/app/workspace'

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

type CyclePayload = ReturnType<typeof buildEditPayload>

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
        // A new figure in the series; the old one stays on record. Kilograms as decimal text.
        await farmRecordsHarvestReportsRevise(workspaceId(), context.cycleId, {
          kind: context.harvestKind,
          quantity_kg: values.quantity_kg.trim(),
          source: 'field_verified',
          confidence: values.confidence || null,
          reported_for: values.reported_for || null,
        })
        return
      }

      if (table === 'crop_cycle') {
        // The crop_cycle branch of the payload; TS cannot narrow the union on the table argument.
        const payload = buildEditPayload(table, values, activeMeasure) as Extract<CyclePayload, { crop_id: string }>
        // PUT takes every editable field; the area goes as typed, so no float rounds it.
        await farmRecordsCropCyclesCorrect(workspaceId(), id, {
          ...payload,
          area_ha: payload.area_ha === null ? null : values.area_ha.trim(),
        })
        return
      }

      // shortcut: person, household, farm and plot still correct through Supabase; their screens are gated
      // until the port reaches them (person detail, farm detail), when this moves to PUT /farm/{kind}/{id}.
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
