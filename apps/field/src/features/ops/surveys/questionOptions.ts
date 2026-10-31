import type { Database } from '@/lib/db.types'

export type QuestionKind = Database['public']['Enums']['survey_question_kind']

export const QUESTION_KINDS: QuestionKind[] = ['single_choice', 'multi_choice', 'yes_no', 'number', 'text']

/** One option as the editor holds it. Swahili is optional, so '' when absent. */
export interface EditableOption {
  value: string
  label_en: string
  label_sw: string
}

/**
 * One option as `survey_question.options` stores it. A type alias rather than
 * an interface so an array of them is assignable to the generated `Json`.
 */
export type StoredOption = {
  value: string
  label_en: string
  label_sw?: string
}

export function isChoiceKind(kind: QuestionKind): boolean {
  return kind === 'single_choice' || kind === 'multi_choice'
}

/** The jsonb array, read defensively: it is authored data, not a type. */
export function parseOptions(options: unknown): EditableOption[] {
  if (!Array.isArray(options)) return []
  return options.flatMap((option) => {
    if (!option || typeof option !== 'object' || Array.isArray(option)) return []
    const o = option as Record<string, unknown>
    return [
      {
        value: typeof o.value === 'string' ? o.value : '',
        label_en: typeof o.label_en === 'string' ? o.label_en : '',
        label_sw: typeof o.label_sw === 'string' ? o.label_sw : '',
      },
    ]
  })
}

/**
 * An option's stored `value`, made from its English label.
 *
 * Answers are recorded against the value and tallied by it, so a value is
 * made ONCE — when a labelled option is first saved — and never regenerated
 * after. Editing a label later changes what a farmer reads, not what earlier
 * answers meant.
 */
export function slugValue(label: string, taken: readonly string[]): string {
  const trimmed = label.trim()
  if (trimmed === '') return ''
  const base =
    trimmed
      .toLowerCase()
      .normalize('NFKD')
      .replace(/[^a-z0-9]+/g, '_')
      .replace(/^_+|_+$/g, '') || 'option'
  if (!taken.includes(base)) return base
  let n = 2
  while (taken.includes(`${base}_${n}`)) n += 1
  return `${base}_${n}`
}

/**
 * The options array to send.
 *
 * Only choice kinds carry options. Nothing is dropped or refused here: a
 * blank option is sent as it stands, and survey_guard's "every option needs a
 * value and an English label" answers it at publish.
 */
export function optionsForSave(kind: QuestionKind, options: EditableOption[]): StoredOption[] {
  if (!isChoiceKind(kind)) return []
  const taken = options.map((o) => o.value).filter((v) => v !== '')
  return options.map((option) => {
    const label_en = option.label_en.trim()
    const label_sw = option.label_sw.trim()
    let value = option.value
    if (value === '') {
      value = slugValue(label_en, taken)
      if (value !== '') taken.push(value)
    }
    return label_sw === '' ? { value, label_en } : { value, label_en, label_sw }
  })
}
