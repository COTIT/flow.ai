import type { Database } from '@/lib/db.types'

export type EditableTable = 'person' | 'household' | 'farm' | 'plot' | 'crop_cycle' | 'harvest_report'
export type CropMeasure = Database['public']['Enums']['crop_measure']
export type EditValues = Record<string, string>

export interface EditContext {
  villageId: string
  personId?: string
  farmId?: string
  cycleId?: string
  harvestKind?: Database['public']['Enums']['harvest_kind']
}

export function selectedEditMeasure(
  table: EditableTable,
  values: EditValues,
  measureByCrop: Readonly<Record<string, CropMeasure>> | undefined,
  fallback: CropMeasure | undefined,
) {
  if (table !== 'crop_cycle') return fallback
  if (!measureByCrop) return fallback
  return measureByCrop[values.crop_id ?? ''] ?? fallback
}

const numberValue = (values: EditValues, key: string, min: number, max: number, integer = false) => {
  const raw = values[key]?.trim() ?? ''
  if (!raw) return null
  const value = Number(raw)
  if (!Number.isFinite(value)) return `${key} must be a number`
  if (integer && !Number.isInteger(value)) return `${key} must be a whole number`
  if (value < min || value > max) return `${key} is outside the allowed range`
  return null
}

const required = (values: EditValues, key: string) =>
  values[key]?.trim() ? null : `${key} is required`

export function validateEdit(
  table: EditableTable,
  values: EditValues,
  measure?: CropMeasure,
): string | null {
  if (table === 'person') return required(values, 'given_name') ?? required(values, 'family_name')
  if (table === 'household' || table === 'farm' || table === 'plot') {
    const textError = required(values, 'label')
    if (textError) return textError
  }
  if (table === 'farm' || table === 'plot') {
    return (
      numberValue(values, 'latitude', -90, 90) ??
      numberValue(values, 'longitude', -180, 180) ??
      (table === 'plot' ? numberValue(values, 'area_ha', 0, 999999.9999) : null)
    )
  }
  if (table === 'crop_cycle') {
    if (!values.crop_id?.trim()) return 'crop_id is required'
    if (!measure) return 'crop measure is unavailable'
    const measureError =
      measure === 'area'
        ? numberValue(values, 'area_ha', 0, 999999.9999)
        : measure === 'tree_count'
          ? numberValue(values, 'tree_count', 0, 2147483647, true)
          : numberValue(values, 'unit_count', 0, 2147483647, true)
    if (measureError) return measureError
    if (values.harvest_start && values.harvest_end && values.harvest_end < values.harvest_start) {
      return 'harvest window must end on or after it starts'
    }
  }
  if (table === 'harvest_report') return numberValue(values, 'quantity_kg', 0, 9999999999.99)
  return null
}

const optionalNumber = (values: EditValues, key: string) => {
  const raw = values[key]?.trim() ?? ''
  return raw === '' ? null : Number(raw)
}

export function buildEditPayload(
  table: Exclude<EditableTable, 'harvest_report'>,
  values: EditValues,
  measure?: CropMeasure,
) {
  if (table === 'person') {
    return {
      given_name: values.given_name.trim(),
      family_name: values.family_name.trim(),
      phone: values.phone?.trim() ?? '',
    }
  }
  if (table === 'household') return { label: values.label.trim() }
  if (table === 'farm') {
    return {
      label: values.label.trim(),
      latitude: optionalNumber(values, 'latitude'),
      longitude: optionalNumber(values, 'longitude'),
    }
  }
  if (table === 'plot') {
    return {
      label: values.label.trim(),
      area_ha: optionalNumber(values, 'area_ha'),
      latitude: optionalNumber(values, 'latitude'),
      longitude: optionalNumber(values, 'longitude'),
    }
  }
  // The dialog hides the measures a crop does not use but keeps their old
  // values; the RPC refuses a non-matching measure, so drop them when known.
  const keeps = (field: CropMeasure) => !measure || measure === field
  return {
    crop_id: values.crop_id,
    season_label: values.season_label?.trim() ?? '',
    area_ha: keeps('area') ? optionalNumber(values, 'area_ha') : null,
    tree_count: keeps('tree_count') && values.tree_count?.trim() ? Number(values.tree_count) : null,
    unit_count: keeps('unit_count') && values.unit_count?.trim() ? Number(values.unit_count) : null,
    planted_on: values.planted_on,
    harvest_start: values.harvest_start,
    harvest_end: values.harvest_end,
    status: values.status,
  }
}
