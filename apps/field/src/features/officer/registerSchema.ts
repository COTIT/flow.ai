import { z } from 'zod'

import type { CycleForm, RegisterForm } from '@/features/officer/registerPayload'
import type { Database } from '@/lib/db.types'

export type CropMeasure = Database['public']['Enums']['crop_measure']

/**
 * The register form's validation — spec 5.2, QA #16, #19, #21, #27.
 *
 * **What belongs here and what does not.** CLAUDE.md forbids pre-validating
 * rules the database enforces, and that rule is about BUSINESS logic: the
 * status machine, the over-commitment guard, who may verify what. A copy of
 * one of those drifts within a fortnight and the copy is what the user sees.
 *
 * None of that is what this file does. It checks that a number is a number,
 * that a name is not three spaces, that an end date follows a start date, and
 * that a value fits the column it is going into. Those are properties of the
 * form's own inputs, and the alternative is not "the database decides" — it is
 * a round trip that returns `numeric field overflow` naming no field, or a
 * constraint identifier rendered as user copy.
 *
 * Every message is an i18n key, resolved at render, like `LoginScreen`.
 *
 * The RPC still owns everything real. It nullifs blanks itself
 * (`nullif(cy->>'area_ha','')::hectares`), so blanks stay blank here rather
 * than becoming zeros, and its own messages still come back verbatim.
 */

/** A required free-text field: trimmed, and not empty once trimmed. */
const requiredText = z
  .string()
  .transform((s) => s.trim())
  .refine((s) => s.length > 0, 'register.required')

/** An optional free-text field. Trimmed so `"   "` is stored as nothing. */
const optionalText = z.string().transform((s) => s.trim())

interface NumberRules {
  /** Largest value the column will hold — `numeric(p,s)` gives 10^(p−s) − 1. */
  max: number
  min?: number
  /** Message key when outside the range. Defaults differ by end. */
  belowKey?: string
  aboveKey?: string
  integer?: boolean
  /** Blank is an error rather than "not answered". */
  required?: boolean
}

/**
 * An optional numeric field, as text.
 *
 * Stays a string all the way through: the RPC takes jsonb and decides what a
 * blank means, and converting to a number here would turn "" into 0 — a
 * measured zero where the officer answered nothing.
 */
function numericText(rules: NumberRules) {
  const {
    max,
    min = 0,
    belowKey = 'register.notNegative',
    aboveKey = 'register.tooLarge',
    integer = false,
    required = false,
  } = rules

  return z
    .string()
    .transform((s) => s.trim())
    .superRefine((value, ctx) => {
      if (value === '') {
        if (required) ctx.addIssue({ code: 'custom', message: 'register.required' })
        return
      }

      const n = Number(value)
      if (!Number.isFinite(n)) {
        ctx.addIssue({ code: 'custom', message: 'register.notANumber' })
        return
      }
      if (integer && !Number.isInteger(n)) {
        ctx.addIssue({ code: 'custom', message: 'register.wholeNumber' })
        return
      }
      if (n < min) ctx.addIssue({ code: 'custom', message: belowKey })
      if (n > max) ctx.addIssue({ code: 'custom', message: aboveKey })
    })
}

/** `hectares` is `numeric(10,4) check (value >= 0)`. */
const hectares = numericText({ max: 999_999.9999 })
/** `harvest_report.quantity_kg` is `numeric(12,2) check (quantity_kg >= 0)`. */
const quantityKg = numericText({ max: 9_999_999_999.99 })
/** `plot.latitude` / `longitude` are `numeric(9,6)` with their own ranges. */
// One message for either end: "-95" and "95" are the same mistake, and
// "too large" is the wrong word for a latitude. Required: a new farm is
// located by the officer's GPS read, and app_register_farmer refuses one
// without it.
const latitude = numericText({
  max: 90,
  min: -90,
  belowKey: 'register.latitudeRange',
  aboveKey: 'register.latitudeRange',
  required: true,
})
const longitude = numericText({
  max: 180,
  min: -180,
  belowKey: 'register.longitudeRange',
  aboveKey: 'register.longitudeRange',
  required: true,
})
const wholeCount = numericText({ max: 2_147_483_647, integer: true })

const cycle = z.object({
  crop_id: z.string().min(1, 'register.required'),
  area_ha: hectares,
  tree_count: wholeCount,
  unit_count: wholeCount,
  harvest_start: z.string(),
  harvest_end: z.string(),
  harvest_quantity_kg: quantityKg,
})

const base = z.object({
  given_name: requiredText,
  family_name: requiredText,
  // The farmer's login name. Its format is app_normalize_phone's call.
  phone: requiredText,
  household_label: optionalText,
  is_head: z.boolean(),
  farm_label: requiredText,
  farm_latitude: latitude,
  farm_longitude: longitude,
  plot_label: requiredText,
  plot_area_ha: hectares,
  season_label: optionalText,
  planted_on: z.string(),
  cycles: z.array(cycle).min(1, 'register.cropsRequired'),
  confidence: z.enum(['low', 'medium', 'high']),
})

/** Which field each crop's own `measured_by` makes mandatory. */
const MEASURE_FIELD = {
  area: 'area_ha',
  tree_count: 'tree_count',
  unit_count: 'unit_count',
} as const satisfies Record<CropMeasure, keyof z.infer<typeof cycle>>

/**
 * The schema, for the crops on offer.
 *
 * A factory rather than one static schema because each crop's mandatory
 * measure field depends on that crop's `measured_by`, which arrives with the
 * crop list. A crop whose measure is unknown demands none: the RPC still
 * decides, and its message names the measure.
 */
export function registerSchema(measures: Readonly<Record<string, CropMeasure>>) {
  return base.superRefine((form, ctx) => {
    form.cycles.forEach((c, index) => {
      const measure = measures[c.crop_id]
      if (measure) {
        const field = MEASURE_FIELD[measure]
        if (c[field] === '') {
          ctx.addIssue({ code: 'custom', path: ['cycles', index, field], message: 'register.required' })
        }
      }

      // `cycle_window_sane`: harvest_end >= harvest_start, either may be null.
      // Reported on the end date, which is the one that is wrong.
      if (c.harvest_start && c.harvest_end && c.harvest_end < c.harvest_start) {
        ctx.addIssue({
          code: 'custom',
          path: ['cycles', index, 'harvest_end'],
          message: 'register.windowBackwards',
        })
      }
    })
  })
}

/**
 * The value a column will actually store, when that differs from what was
 * typed — QA #27.
 *
 * `hectares` is `numeric(10,4)`, so `1.23456789` is stored as `1.2346`:
 * correct, and silent. Returns null when nothing changes, so a correct entry
 * gets no notice. Trailing zeros are not a change.
 */
export function roundedTo(value: string, dp: number): string | null {
  const text = value.trim()
  if (text === '') return null

  const n = Number(text)
  if (!Number.isFinite(n)) return null

  const rounded = n.toFixed(dp)
  return Number(rounded) === n ? null : rounded
}

/** Every top-level field a `RegisterForm` holds, with the type it must hold. */
const DRAFT_FIELDS = {
  given_name: 'string',
  family_name: 'string',
  phone: 'string',
  household_label: 'string',
  is_head: 'boolean',
  farm_label: 'string',
  farm_latitude: 'string',
  farm_longitude: 'string',
  plot_label: 'string',
  plot_area_ha: 'string',
  season_label: 'string',
  planted_on: 'string',
  confidence: 'string',
} as const

const CYCLE_FIELDS = [
  'crop_id',
  'area_ha',
  'tree_count',
  'unit_count',
  'harvest_start',
  'harvest_end',
  'harvest_quantity_kg',
] as const satisfies readonly (keyof CycleForm)[]

/** The crop fields of the one-crop form (before 5 Oct 2026), flat on the draft. */
const LEGACY_CYCLE_FIELDS = {
  crop_id: 'crop_id',
  cycle_area_ha: 'area_ha',
  cycle_tree_count: 'tree_count',
  cycle_unit_count: 'unit_count',
  harvest_start: 'harvest_start',
  harvest_end: 'harvest_end',
  harvest_quantity_kg: 'harvest_quantity_kg',
} as const satisfies Record<string, keyof CycleForm>

const CONFIDENCE = ['low', 'medium', 'high']

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

const isCycle = (value: unknown) =>
  isRecord(value) && CYCLE_FIELDS.every((field) => typeof value[field] === 'string')

const isLegacyCycles = (draft: Record<string, unknown>) =>
  Object.keys(LEGACY_CYCLE_FIELDS).every((field) => typeof draft[field] === 'string')

/**
 * Is this stored object still a draft of THIS form? — QA #22.
 *
 * Shape only, deliberately. A half-filled draft is the whole point of the
 * feature, so an empty required field restores; what must not restore is an
 * object whose fields are the wrong TYPE, which rendered as the literal
 * `[object Object]` in the first name and would have been submitted.
 *
 * The realistic producer is a draft written by an older deployment of the
 * form, on a phone that was mid-registration when the app updated. An extra
 * key is therefore tolerated — a field this version dropped costs nothing —
 * while a missing or retyped one is not. A draft of the one-crop form is
 * accepted too, and `toRegisterForm` turns its crop into the first of the list.
 */
export function isRegisterDraft(value: unknown): value is RegisterForm {
  if (!isRecord(value)) return false

  for (const [field, type] of Object.entries(DRAFT_FIELDS)) {
    if (typeof value[field] !== type) return false
  }
  if (!CONFIDENCE.includes(value.confidence as string)) return false

  if ('cycles' in value) return Array.isArray(value.cycles) && value.cycles.every(isCycle)
  return isLegacyCycles(value)
}

/** A draft of the one-crop form (before 5 Oct 2026). */
export type LegacyRegisterDraft = Omit<RegisterForm, 'cycles'> &
  Record<keyof typeof LEGACY_CYCLE_FIELDS, string>

/** A draft that passed `isRegisterDraft`, in the shape the form holds today. */
export function toRegisterForm(draft: RegisterForm | LegacyRegisterDraft): RegisterForm {
  const raw = draft as unknown as Record<string, unknown>
  if (Array.isArray(raw.cycles)) return draft as RegisterForm

  const rest = { ...raw }
  const cycle = {} as CycleForm
  for (const [legacy, field] of Object.entries(LEGACY_CYCLE_FIELDS)) {
    cycle[field] = raw[legacy] as string
    delete rest[legacy]
  }
  return { ...(rest as Omit<RegisterForm, 'cycles'>), cycles: cycle.crop_id === '' ? [] : [cycle] }
}
