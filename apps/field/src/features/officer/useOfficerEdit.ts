import { useMutation, useQueryClient } from '@tanstack/react-query'
import {
  farmRecordsCropCyclesCorrect,
  farmRecordsFarmsCorrect,
  farmRecordsHarvestReportsRevise,
  farmRecordsHouseholdsCorrect,
  farmRecordsPersonsCorrect,
  farmRecordsPlotsCorrect,
} from '@genesy/api-client/endpoints/farm-records'

import { workspaceId } from '@/app/workspace'

import {
  buildCyclePayload,
  type CropMeasure,
  type EditContext,
  type EditableTable,
  type EditValues,
  validateEdit,
} from '@/features/officer/officerEdit'
import { queryKeys } from '@/lib/queryKeys'
import type { Enums } from '@/lib/enums'

export function useOfficerEdit(measure?: Enums['crop_measure']) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({
      table,
      id,
      values,
      context,
      measure: selectedMeasure,
      clientRef,
    }: {
      table: EditableTable
      id: string
      values: EditValues
      context: EditContext
      measure?: CropMeasure
      /** The edit draft's reference; the API files a harvest figure once per reference. */
      clientRef?: string
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
          ...(clientRef ? { client_ref: clientRef } : {}),
        })
        return
      }

      if (table === 'crop_cycle') {
        const payload = buildCyclePayload(values, activeMeasure)
        // PUT takes every editable field; the area goes as typed, so no float rounds it.
        await farmRecordsCropCyclesCorrect(workspaceId(), id, {
          ...payload,
          area_ha: payload.area_ha === null ? null : values.area_ha.trim(),
        })
        return
      }

      // PUT /farm/{kind}/{id} takes every editable field; numbers go as typed text, blank as null.
      const text = (key: string) => values[key]?.trim() || null
      const workspace = workspaceId()
      if (table === 'person') {
        await farmRecordsPersonsCorrect(workspace, id, {
          given_name: values.given_name.trim(),
          family_name: values.family_name.trim(),
          phone: text('phone'),
        })
      } else if (table === 'household') {
        await farmRecordsHouseholdsCorrect(workspace, id, { label: values.label.trim() })
      } else if (table === 'farm') {
        await farmRecordsFarmsCorrect(workspace, id, { label: values.label.trim(), latitude: text('latitude'), longitude: text('longitude') })
      } else {
        await farmRecordsPlotsCorrect(workspace, id, {
          label: values.label.trim(),
          area_ha: text('area_ha'),
          latitude: text('latitude'),
          longitude: text('longitude'),
        })
      }
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
