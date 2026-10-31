import { closesOnToTimestamp, timestampToDarDate } from '@/features/ops/surveys/surveyDates'
import type { Survey } from '@/features/ops/surveys/useSurveyAdmin'

/**
 * The draft editor's form, as strings — usePersistentForm stores strings —
 * and the patch it sends.
 *
 * Conversion only. Nothing is refused here: `reward_amount > 0`, the
 * audit-rate range and the not-null columns belong to the database, and a
 * blank is sent as null so that the database's answer is what the admin
 * reads (CLAUDE.md §5).
 */

export type SurveyFormFields = Pick<
  Survey,
  | 'title_en'
  | 'title_sw'
  | 'description_en'
  | 'description_sw'
  | 'reward_amount'
  | 'max_households'
  | 'audit_rate'
  | 'closes_at'
>

export interface SurveyFormValues extends Record<string, string> {
  title_en: string
  title_sw: string
  description_en: string
  description_sw: string
  reward_amount: string
  max_households: string
  /** A percent in the form; a fraction (0..1) in the column. */
  audit_rate: string
  /** A Tanzanian day in the form; a timestamptz in the column. */
  closes_on: string
}

/** The same columns, with null where the form was blank. */
export type SurveyFormPatch = {
  title_en: string
  title_sw: string | null
  description_en: string | null
  description_sw: string | null
  reward_amount: number | null
  max_households: number | null
  audit_rate: number | null
  closes_at: string | null
}

/** A number as a form shows it: `2500.00` from PostgREST reads as `2500`. */
function numberText(value: number | string | null): string {
  if (value === null || value === '') return ''
  const n = Number(value)
  return Number.isFinite(n) ? String(n) : ''
}

/**
 * The stored fraction as a percent. Two decimals of the fraction are all the
 * column holds (numeric(3,2)), so rounding to that removes only the floating
 * point residue: 0.15 * 100 is 15.000000000000002.
 */
function percentText(rate: number | string | null): string {
  if (rate === null || rate === '') return ''
  const n = Number(rate)
  return Number.isFinite(n) ? String(Number((n * 100).toFixed(2))) : ''
}

export function surveyFormValues(survey: SurveyFormFields): SurveyFormValues {
  return {
    title_en: survey.title_en,
    title_sw: survey.title_sw ?? '',
    description_en: survey.description_en ?? '',
    description_sw: survey.description_sw ?? '',
    reward_amount: numberText(survey.reward_amount),
    max_households: numberText(survey.max_households),
    audit_rate: percentText(survey.audit_rate),
    closes_on: timestampToDarDate(survey.closes_at),
  }
}

const blankToNull = (value: string) => (value.trim() === '' ? null : value.trim())
const numberOrNull = (value: string) => (value.trim() === '' ? null : Number(value))

export function surveyPatch(values: SurveyFormValues): SurveyFormPatch {
  return {
    title_en: values.title_en.trim(),
    title_sw: blankToNull(values.title_sw),
    description_en: blankToNull(values.description_en),
    description_sw: blankToNull(values.description_sw),
    reward_amount: numberOrNull(values.reward_amount),
    max_households: numberOrNull(values.max_households),
    audit_rate: values.audit_rate.trim() === '' ? null : Number(values.audit_rate) / 100,
    closes_at: closesOnToTimestamp(values.closes_on),
  }
}
