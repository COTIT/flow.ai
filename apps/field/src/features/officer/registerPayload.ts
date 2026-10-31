import type { Database } from '@/lib/db.types'

type CropMeasure = Database['public']['Enums']['crop_measure']
type Confidence = Database['public']['Enums']['confidence_level']

/** One crop grown on the plot: its measure, its harvest window, its expected kg. */
export interface CycleForm {
  crop_id: string
  area_ha: string
  tree_count: string
  unit_count: string
  harvest_start: string
  harvest_end: string
  harvest_quantity_kg: string
}

/**
 * The register form, as strings.
 *
 * Kept as strings deliberately: the RPC takes jsonb and nullifs empty text
 * itself (`nullif(cy->>'area_ha','')::hectares`), so the client does not
 * second-guess which blanks mean null.
 *
 * `cycles` holds one entry per crop ticked. A plot carries several concurrent
 * cycles (intercropping), which is why planted area across cycles can exceed
 * the plot's own area.
 */
export interface RegisterForm {
  given_name: string
  family_name: string
  phone: string
  household_label: string
  is_head: boolean
  farm_label: string
  farm_latitude: string
  farm_longitude: string
  plot_label: string
  plot_area_ha: string
  season_label: string
  planted_on: string
  cycles: CycleForm[]
  confidence: Confidence
}

export interface CyclePayload {
  crop_id: string
  season_label: string
  area_ha: string
  tree_count: string
  unit_count: string
  planted_on: string
  harvest_start: string
  harvest_end: string
  status: string
  confidence: Confidence
  harvest: { quantity_kg: string; reported_for: string; confidence: Confidence }
}

export interface RegisterPayload {
  client_ref: string
  village_id: string
  person: { given_name: string; family_name: string; phone: string; confidence: Confidence }
  household: { label: string; is_head: boolean }
  farm: { label: string; latitude: string; longitude: string; confidence: Confidence }
  plot: { label: string; area_ha: string; confidence: Confidence }
  cycles: CyclePayload[]
}

/**
 * Shapes the form into the RPC's jsonb payload.
 *
 * Carries NO provenance. app_register_farmer stamps source = 'field_verified'
 * and captured_by = auth.uid() itself, which is what keeps an officer from
 * asserting who captured a record.
 *
 * Each crop sends only the measure matching its `crop.measured_by`. The RPC
 * raises "this crop is measured by area: area_ha is required" and its
 * siblings, so sending the wrong one is a guaranteed round trip to a known
 * failure — and a crop with no known measure is refused here for the same
 * reason.
 */
export function buildRegisterPayload(
  form: RegisterForm,
  context: { clientRef: string; villageId: string; measures: Readonly<Record<string, CropMeasure>> },
): RegisterPayload {
  const { clientRef, villageId, measures } = context

  return {
    client_ref: clientRef,
    village_id: villageId,
    person: {
      given_name: form.given_name,
      family_name: form.family_name,
      phone: form.phone,
      confidence: form.confidence,
    },
    household: {
      label: form.household_label,
      is_head: form.is_head,
    },
    farm: {
      label: form.farm_label,
      latitude: form.farm_latitude,
      longitude: form.farm_longitude,
      confidence: form.confidence,
    },
    plot: {
      label: form.plot_label,
      area_ha: form.plot_area_ha,
      confidence: form.confidence,
    },
    cycles: form.cycles.map((cycle) => {
      const measure = measures[cycle.crop_id]
      if (!measure) throw new Error(`unknown crop: ${cycle.crop_id}`)

      return {
        crop_id: cycle.crop_id,
        season_label: form.season_label,
        area_ha: measure === 'area' ? cycle.area_ha : '',
        tree_count: measure === 'tree_count' ? cycle.tree_count : '',
        unit_count: measure === 'unit_count' ? cycle.unit_count : '',
        planted_on: form.planted_on,
        harvest_start: cycle.harvest_start,
        harvest_end: cycle.harvest_end,
        // Matches the RPC's own default for a registration.
        status: 'growing',
        confidence: form.confidence,
        harvest: {
          quantity_kg: cycle.harvest_quantity_kg,
          reported_for: cycle.harvest_start,
          confidence: form.confidence,
        },
      }
    }),
  }
}
