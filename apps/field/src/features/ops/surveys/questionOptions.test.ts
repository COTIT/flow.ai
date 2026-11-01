import { describe, expect, test } from 'vitest'

import {
  isChoiceKind,
  optionsForSave,
  parseOptions,
  slugValue,
} from '@/features/ops/surveys/questionOptions'

describe('isChoiceKind', () => {
  test('only single and multi choice carry options', () => {
    expect(isChoiceKind('single_choice')).toBe(true)
    expect(isChoiceKind('multi_choice')).toBe(true)
    expect(isChoiceKind('yes_no')).toBe(false)
    expect(isChoiceKind('number')).toBe(false)
    expect(isChoiceKind('text')).toBe(false)
  })
})

describe('parseOptions', () => {
  test('reads the jsonb array the question stores', () => {
    expect(
      parseOptions([
        { value: 'maize', label_en: 'Maize', label_sw: 'Mahindi' },
        { value: 'beans', label_en: 'Beans' },
      ]),
    ).toEqual([
      { value: 'maize', label_en: 'Maize', label_sw: 'Mahindi' },
      { value: 'beans', label_en: 'Beans', label_sw: '' },
    ])
  })

  test('anything that is not an array of objects is no options', () => {
    expect(parseOptions(null)).toEqual([])
    expect(parseOptions('maize')).toEqual([])
    expect(parseOptions({ value: 'x' })).toEqual([])
    expect(parseOptions([1, 'two', null])).toEqual([])
  })
})

describe('slugValue', () => {
  test('is a lower-case slug of the English label', () => {
    expect(slugValue('Maize flour', [])).toBe('maize_flour')
    expect(slugValue('  Sunflower (seed)!  ', [])).toBe('sunflower_seed')
  })

  test('never repeats a value the question already holds', () => {
    expect(slugValue('Maize', ['maize'])).toBe('maize_2')
    expect(slugValue('Maize', ['maize', 'maize_2'])).toBe('maize_3')
  })

  test('a label with nothing sluggable still gets a value', () => {
    expect(slugValue('???', [])).toBe('option')
    expect(slugValue('', [])).toBe('')
  })
})

describe('optionsForSave', () => {
  test('a value, once set, never changes when the label is edited', () => {
    expect(
      optionsForSave('single_choice', [{ value: 'maize', label_en: 'Maize grain', label_sw: '' }]),
    ).toEqual([{ value: 'maize', label_en: 'Maize grain' }])
  })

  test('a new option takes its value from its label', () => {
    expect(
      optionsForSave('multi_choice', [
        { value: 'maize', label_en: 'Maize', label_sw: 'Mahindi' },
        { value: '', label_en: 'Maize', label_sw: '' },
        { value: '', label_en: 'Beans ', label_sw: ' ' },
      ]),
    ).toEqual([
      { value: 'maize', label_en: 'Maize', label_sw: 'Mahindi' },
      { value: 'maize_2', label_en: 'Maize' },
      { value: 'beans', label_en: 'Beans' },
    ])
  })

  // survey_guard owns "every option needs a value and an English label": a
  // blank option is sent as it is and refused at publish with that message.
  test('a blank option is sent blank, not dropped or pre-checked', () => {
    expect(optionsForSave('single_choice', [{ value: '', label_en: '  ', label_sw: '' }])).toEqual([
      { value: '', label_en: '' },
    ])
  })

  test('a kind without options saves none', () => {
    const options = [{ value: 'maize', label_en: 'Maize', label_sw: '' }]
    expect(optionsForSave('yes_no', options)).toEqual([])
    expect(optionsForSave('number', options)).toEqual([])
    expect(optionsForSave('text', options)).toEqual([])
  })
})
