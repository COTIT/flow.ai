import type { Money } from '@genesy/api-client/model'

import { amountToMinor, minorToAmount } from '@/features/ops/surveys/figures'
import { closesOnToTimestamp, timestampToDarDate } from '@/features/ops/surveys/surveyDates'
import type { Survey } from '@/features/ops/surveys/useSurveyAdmin'

/**
 * The draft editor's form, as strings — usePersistentForm stores strings —
 * and the PATCH body it sends.
 *
 * Conversion only. Nothing is refused here: `reward_minor > 0`, the audit
 * range and the required fields belong to the API, and a blank is sent as
 * null so that the API's answer is what the admin reads (CLAUDE.md §5).
 */

export type SurveyFormFields = Pick<
  Survey,
  'title_en' | 'title_sw' | 'description_en' | 'description_sw' | 'reward' | 'max_households' | 'audit_percent' | 'closes_at'
>

export interface SurveyFormValues extends Record<string, string> {
  title_en: string
  title_sw: string
  description_en: string
  description_sw: string
  /** Major units as typed ("5,000"); integer minor units on the wire. */
  reward_amount: string
  max_households: string
  /** A whole percent, 0–100. */
  audit_rate: string
  /** A Tanzanian day in the form; an RFC 3339 time on the wire. */
  closes_on: string
}

/** The PATCH body, with null where the form was blank. */
export type SurveyFormPatch = {
  title_en: string
  title_sw: string | null
  description_en: string | null
  description_sw: string | null
  reward_minor: number | null
  currency_id: string
  max_households: number | null
  audit_percent: number | null
  closes_at: string | null
}

const text = (value: number | null) => (value === null ? '' : String(value))

export function surveyFormValues(survey: SurveyFormFields): SurveyFormValues {
  return {
    title_en: survey.title_en,
    title_sw: survey.title_sw ?? '',
    description_en: survey.description_en ?? '',
    description_sw: survey.description_sw ?? '',
    reward_amount: minorToAmount(survey.reward.minor, survey.reward.decimals),
    max_households: text(survey.max_households),
    audit_rate: text(survey.audit_percent),
    closes_on: timestampToDarDate(survey.closes_at),
  }
}

const blankToNull = (value: string) => (value.trim() === '' ? null : value.trim())
const numberOrNull = (value: string) => (value.trim() === '' ? null : Number(value))

/** `reward` is the survey's stored one: its currency and decimals. */
export function surveyPatch(values: SurveyFormValues, reward: Pick<Money, 'currency_id' | 'decimals'>): SurveyFormPatch {
  return {
    title_en: values.title_en.trim(),
    title_sw: blankToNull(values.title_sw),
    description_en: blankToNull(values.description_en),
    description_sw: blankToNull(values.description_sw),
    reward_minor: amountToMinor(values.reward_amount, reward.decimals),
    currency_id: reward.currency_id,
    max_households: numberOrNull(values.max_households),
    audit_percent: numberOrNull(values.audit_rate),
    closes_at: closesOnToTimestamp(values.closes_on),
  }
}
