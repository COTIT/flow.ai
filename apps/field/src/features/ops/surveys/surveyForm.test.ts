import { describe, expect, test } from 'vitest'

import { surveyFormValues, surveyPatch } from '@/features/ops/surveys/surveyForm'

const survey = {
  title_en: 'Harvest intentions',
  title_sw: null,
  description_en: 'What you plan to grow',
  description_sw: null,
  reward_amount: 5000,
  max_households: null,
  audit_rate: 0.15,
  closes_at: '2026-10-31T20:59:59+00:00',
}

describe('surveyFormValues', () => {
  test('the form starts from the stored survey, with absent values blank', () => {
    expect(surveyFormValues(survey)).toEqual({
      title_en: 'Harvest intentions',
      title_sw: '',
      description_en: 'What you plan to grow',
      description_sw: '',
      reward_amount: '5000',
      max_households: '',
      audit_rate: '15',
      closes_on: '2026-10-31',
    })
  })

  // 0.15 * 100 is 15.000000000000002 in floating point; the form shows 15.
  test('the audit rate is shown as a clean percent', () => {
    expect(surveyFormValues({ ...survey, audit_rate: 0.07 }).audit_rate).toBe('7')
    expect(surveyFormValues({ ...survey, audit_rate: 0.29 }).audit_rate).toBe('29')
    expect(surveyFormValues({ ...survey, audit_rate: 0 }).audit_rate).toBe('0')
    expect(surveyFormValues({ ...survey, audit_rate: 1 }).audit_rate).toBe('100')
  })

  test('PostgREST numerics that arrive as strings still read', () => {
    const values = surveyFormValues({ ...survey, reward_amount: '2500.00' as unknown as number, audit_rate: '0.20' as unknown as number })
    expect(values.reward_amount).toBe('2500')
    expect(values.audit_rate).toBe('20')
  })
})

describe('surveyPatch', () => {
  const values = surveyFormValues(survey)

  test('sends the draft columns only, converted back', () => {
    expect(surveyPatch(values)).toEqual({
      title_en: 'Harvest intentions',
      title_sw: null,
      description_en: 'What you plan to grow',
      description_sw: null,
      reward_amount: 5000,
      max_households: null,
      audit_rate: 0.15,
      closes_at: '2026-10-31T23:59:59+03:00',
    })
  })

  test('blank optional text is absent, not an empty string', () => {
    const patch = surveyPatch({ ...values, title_sw: '  ', description_en: '' })
    expect(patch.title_sw).toBeNull()
    expect(patch.description_en).toBeNull()
  })

  test('typed Swahili is kept, trimmed', () => {
    expect(surveyPatch({ ...values, title_sw: ' Nia ya mavuno ' }).title_sw).toBe('Nia ya mavuno')
  })

  test('a whole-percent audit rate becomes the stored fraction exactly', () => {
    expect(surveyPatch({ ...values, audit_rate: '7' }).audit_rate).toBe(0.07)
    expect(surveyPatch({ ...values, audit_rate: '100' }).audit_rate).toBe(1)
  })

  test('most households is a whole number or nothing', () => {
    expect(surveyPatch({ ...values, max_households: '40' }).max_households).toBe(40)
    expect(surveyPatch({ ...values, max_households: '' }).max_households).toBeNull()
  })

  // The database owns "reward_amount > 0", the audit_rate range and its
  // not-null columns. The form sends what was typed and shows the answer.
  test('nothing is pre-checked: blanks go as null, out-of-range values as typed', () => {
    const patch = surveyPatch({ ...values, reward_amount: '', audit_rate: '', title_en: '  ' })
    expect(patch.reward_amount).toBeNull()
    expect(patch.audit_rate).toBeNull()
    expect(patch.title_en).toBe('')
    expect(surveyPatch({ ...values, reward_amount: '-5', audit_rate: '150' })).toMatchObject({
      reward_amount: -5,
      audit_rate: 1.5,
    })
  })

  test('no closing day is no closing date', () => {
    expect(surveyPatch({ ...values, closes_on: '' }).closes_at).toBeNull()
  })
})
