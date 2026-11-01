import type { RegistrationRequest } from '@genesy/api-client/model'

import type { Enums } from '@/lib/enums'

type CropMeasure = Enums['crop_measure']
type Confidence = Enums['confidence_level']

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
 * The register form, as strings. A blank is sent as null (Genesy refuses `''` for a decimal or a
 * date); counts go as whole numbers.
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

/** The registration body, with every section present (the form always sends all of them). */
export type RegisterPayload = RegistrationRequest & {
  [K in 'person' | 'household' | 'farm' | 'plot' | 'cycles']-?: NonNullable<RegistrationRequest[K]>
}

/** A blank field is no value. */
const orNull = (value: string): string | null => (value === '' ? null : value)
const countOrNull = (value: string): number | null => (value === '' ? null : Number(value))

/**
 * Shapes the form into `POST …/farm/registrations`.
 *
 * Carries NO provenance. The API stamps source = 'field_verified' and
 * captured_by = the caller itself, which is what keeps an officer from
 * asserting who captured a record.
 *
 * Each crop sends only the measure matching its `crop.measured_by`. The API
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
      label: orNull(form.household_label),
      is_head: form.is_head,
    },
    farm: {
      label: form.farm_label,
      latitude: orNull(form.farm_latitude),
      longitude: orNull(form.farm_longitude),
      confidence: form.confidence,
    },
    plot: {
      label: form.plot_label,
      area_ha: orNull(form.plot_area_ha),
      confidence: form.confidence,
    },
    cycles: form.cycles.map((cycle) => {
      const measure = measures[cycle.crop_id]
      if (!measure) throw new Error(`unknown crop: ${cycle.crop_id}`)

      return {
        crop_id: cycle.crop_id,
        season_label: orNull(form.season_label),
        area_ha: measure === 'area' ? orNull(cycle.area_ha) : null,
        tree_count: measure === 'tree_count' ? countOrNull(cycle.tree_count) : null,
        unit_count: measure === 'unit_count' ? countOrNull(cycle.unit_count) : null,
        planted_on: orNull(form.planted_on),
        harvest_start: orNull(cycle.harvest_start),
        harvest_end: orNull(cycle.harvest_end),
        // Matches the API's own default for a registration.
        status: 'growing',
        confidence: form.confidence,
        // The expected harvest is optional: no quantity, no report.
        harvest:
          cycle.harvest_quantity_kg === ''
            ? null
            : {
                quantity_kg: cycle.harvest_quantity_kg,
                reported_for: orNull(cycle.harvest_start),
                confidence: form.confidence,
              },
      }
    }),
  }
}

/** API error keys whose field has an error line on the form. */
const FORM_FIELD: Record<string, string> = {
  'person.given_name': 'given_name',
  'person.family_name': 'family_name',
  'person.phone': 'phone',
  'farm.label': 'farm_label',
  'farm.latitude': 'farm_latitude',
  'farm.longitude': 'farm_longitude',
  'plot.label': 'plot_label',
  'plot.area_ha': 'plot_area_ha',
  cycles: 'cycles.root',
}

/**
 * Where an API 422 key (`person.phone`, `cycles.0.area_ha`) shows on the form, or null when the
 * form has no line for it (the refusal then goes to the general message). Display only: the API
 * decides what is wrong.
 */
export function registerFieldOf(key: string): string | null {
  if (FORM_FIELD[key]) return FORM_FIELD[key]
  const cycle = /^cycles\.(\d+)\.(area_ha|tree_count|unit_count|harvest_start|harvest_end|harvest\.quantity_kg)$/.exec(key)
  if (!cycle) return null
  return `cycles.${cycle[1]}.${cycle[2] === 'harvest.quantity_kg' ? 'harvest_quantity_kg' : cycle[2]}`
}
