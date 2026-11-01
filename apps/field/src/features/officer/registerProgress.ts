import type { CycleForm, RegisterForm } from '@/features/officer/registerPayload'
import type { CropMeasure } from '@/features/officer/registerSchema'

/**
 * The six groups the one-page registration creates, in the order they are
 * created: person → household → farm → plot → crop cycle → expected harvest.
 */
export type RegisterGroup = 'person' | 'household' | 'farm' | 'plot' | 'cycle' | 'harvest'

export const REGISTER_GROUPS: readonly RegisterGroup[] = [
  'person',
  'household',
  'farm',
  'plot',
  'cycle',
  'harvest',
]

type Measures = Readonly<Record<string, CropMeasure>>

/**
 * Which groups have everything they need.
 *
 * This drives the completion rail, and it is the only thing on the screen that
 * says how far along the officer is. It is deliberately NOT validation: the
 * schema decides what may be submitted, and the database decides what is true.
 * A group here is "filled in", nothing stronger — a rail that claimed a section
 * was correct would be making a promise this code cannot keep.
 *
 * `measures` maps each crop to its `measured_by`, so each crop asks for the
 * one measure field it allows and ignores the other two. The crop and harvest
 * groups are complete only when EVERY crop ticked is.
 */
export function isGroupComplete(group: RegisterGroup, form: RegisterForm, measures: Measures): boolean {
  switch (group) {
    case 'person':
      return filled(form.given_name) && filled(form.family_name)
    case 'household':
      return filled(form.household_label)
    case 'farm':
      return filled(form.farm_label)
    case 'plot':
      return filled(form.plot_label) && filled(form.plot_area_ha)
    case 'cycle':
      return (
        form.cycles.length > 0 &&
        form.cycles.every(
          (c) =>
            filled(c.harvest_start) && filled(c.harvest_end) && measureFilled(c, measures[c.crop_id]),
        )
      )
    case 'harvest':
      return form.cycles.length > 0 && form.cycles.every((c) => filled(c.harvest_quantity_kg))
  }
}

const filled = (value: string | undefined) => (value ?? '').trim().length > 0

/** A crop measured by area needs its hectares, and nothing else will do. */
function measureFilled(cycle: CycleForm, measure: CropMeasure | undefined): boolean {
  switch (measure) {
    case 'area':
      return filled(cycle.area_ha)
    case 'tree_count':
      return filled(cycle.tree_count)
    case 'unit_count':
      return filled(cycle.unit_count)
    // A crop the list does not know: there is no measure to ask for.
    default:
      return false
  }
}

export function completedGroups(form: RegisterForm, measures: Measures): RegisterGroup[] {
  return REGISTER_GROUPS.filter((group) => isGroupComplete(group, form, measures))
}
