import { describe, expect, test } from 'vitest'

import { surveyFormValues, surveyPatch } from '@/features/ops/surveys/surveyForm'

const survey = {
  title_en: 'Harvest intentions',
  title_sw: null,
  description_en: 'What you plan to grow',
  description_sw: null,
  reward: { minor: 500000, decimals: 2, currency: 'TZS', currency_id: 'c-tzs' },
  max_households: null,
  audit_percent: 15,
  closes_at: '2026-10-31T20:59:59+00:00',
}

describe('surveyFormValues', () => {
  test('the form starts from the stored survey, with absent values blank', () => {
    expect(surveyFormValues(survey)).toEqual({
      title_en: 'Harvest intentions',
      title_sw: '',
      description_en: 'What you plan to grow',
      description_sw: '',
      reward_amount: '5000.00',
      max_households: '',
      audit_rate: '15',
      closes_on: '2026-10-31',
    })
  })
})

describe('surveyPatch', () => {
  const values = surveyFormValues(survey)

  test('sends the draft fields, the reward as integer minor units of the survey currency', () => {
    expect(surveyPatch(values, survey.reward)).toEqual({
      title_en: 'Harvest intentions',
      title_sw: null,
      description_en: 'What you plan to grow',
      description_sw: null,
      reward_minor: 500000,
      currency_id: 'c-tzs',
      max_households: null,
      audit_percent: 15,
      closes_at: '2026-10-31T23:59:59+03:00',
    })
  })

  test('a typed reward with grouping becomes exact minor units', () => {
    expect(surveyPatch({ ...values, reward_amount: '7,500.50' }, survey.reward).reward_minor).toBe(750050)
  })

  test('blank optional text is absent, not an empty string', () => {
    const patch = surveyPatch({ ...values, title_sw: '  ', description_en: '' }, survey.reward)
    expect(patch.title_sw).toBeNull()
    expect(patch.description_en).toBeNull()
  })

  test('typed Swahili is kept, trimmed', () => {
    expect(surveyPatch({ ...values, title_sw: ' Nia ya mavuno ' }, survey.reward).title_sw).toBe('Nia ya mavuno')
  })

  test('most households is a whole number or nothing', () => {
    expect(surveyPatch({ ...values, max_households: '40' }, survey.reward).max_households).toBe(40)
    expect(surveyPatch({ ...values, max_households: '' }, survey.reward).max_households).toBeNull()
  })

  // The API owns "reward_minor > 0", the audit range and the required fields.
  // The form sends what was typed and shows the answer.
  test('nothing is pre-checked: blanks go as null, out-of-range values as typed', () => {
    const patch = surveyPatch({ ...values, reward_amount: '', audit_rate: '', title_en: '  ' }, survey.reward)
    expect(patch.reward_minor).toBeNull()
    expect(patch.audit_percent).toBeNull()
    expect(patch.title_en).toBe('')
    expect(surveyPatch({ ...values, audit_rate: '150' }, survey.reward).audit_percent).toBe(150)
  })

  test('no closing day is no closing date', () => {
    expect(surveyPatch({ ...values, closes_on: '' }, survey.reward).closes_at).toBeNull()
  })
})
