import { describe, expect, test } from 'vitest'

import {
  isRegisterDraft,
  registerSchema,
  roundedTo,
  toRegisterForm,
} from '@/features/officer/registerSchema'
import type { CycleForm, RegisterForm } from '@/features/officer/registerPayload'

const MAIZE = '40000000-0000-4000-8000-000000000001'
const COFFEE = '40000000-0000-4000-8000-000000000003'
const HONEY = '40000000-0000-4000-8000-000000000005'
const MEASURES = { [MAIZE]: 'area', [COFFEE]: 'tree_count', [HONEY]: 'unit_count' } as const

const MAIZE_CYCLE: CycleForm = {
  crop_id: MAIZE,
  area_ha: '1.6',
  tree_count: '',
  unit_count: '',
  harvest_start: '2026-09-01',
  harvest_end: '2026-09-30',
  harvest_quantity_kg: '4100',
}

const VALID: RegisterForm = {
  given_name: 'Neema',
  family_name: 'Mwakalinga',
  phone: '+255700000101',
  household_label: 'Mwakalinga household',
  is_head: true,
  farm_label: 'Shamba la Neema',
  farm_latitude: '-8.1303',
  farm_longitude: '35.1895',
  plot_label: 'Kipande cha juu',
  plot_area_ha: '1.8',
  season_label: 'Msimu 2026 A',
  planted_on: '2026-03-05',
  cycles: [MAIZE_CYCLE],
  confidence: 'high',
}

const parse = (over: Partial<RegisterForm> = {}) =>
  registerSchema(MEASURES).safeParse({ ...VALID, ...over })

/** The form with its one maize crop changed. */
const parseCycle = (over: Partial<CycleForm>) => parse({ cycles: [{ ...MAIZE_CYCLE, ...over }] })

/** Every message key raised, so a test can assert on the reason not the field. */
const issues = (result: ReturnType<typeof parse>) =>
  result.success ? [] : result.error.issues.map((i) => `${i.path.join('.')}:${i.message}`)

describe('a complete registration', () => {
  test('passes', () => {
    expect(parse().success).toBe(true)
  })

  // Beyond the names, the RPC requires only the phone (the farmer's login
  // name) and the farm GPS position, and nullifs every other blank itself. A
  // schema that demanded more would block registrations the database accepts.
  test('passes with only the fields the RPC actually requires', () => {
    const result = parse({
      household_label: '',
      plot_area_ha: '',
      season_label: '',
      planted_on: '',
      cycles: [{ ...MAIZE_CYCLE, harvest_start: '', harvest_end: '', harvest_quantity_kg: '' }],
    })
    expect(issues(result)).toEqual([])
  })
})

/**
 * QA #16. `required: true` rejects `""` and accepts `"   "`, and
 * `person.given_name` is `not null`, which `'   '` satisfies — so a farmer
 * could be registered with a blank name, on a screen with no rename to fix it.
 */
describe('whitespace is not a value', () => {
  test.each(['given_name', 'family_name', 'farm_label', 'plot_label'] as const)(
    '%s cannot be spaces',
    (field) => {
      const result = parse({ [field]: '   ' } as Partial<RegisterForm>)
      expect(issues(result)).toContain(`${field}:register.required`)
    },
  )

  test.each(['given_name', 'family_name', 'farm_label', 'plot_label'] as const)(
    '%s cannot be empty either',
    (field) => {
      expect(issues(parse({ [field]: '' } as Partial<RegisterForm>))).toContain(
        `${field}:register.required`,
      )
    },
  )

  // Rejecting is half the fix. The stored value must also be the trimmed one,
  // or " Neema " becomes a name nobody can search for.
  test('surrounding space is removed from what gets stored', () => {
    const result = parse({ given_name: '  Neema  ', family_name: ' Mwakalinga ' })
    expect(result.success).toBe(true)
    if (result.success) {
      expect(result.data.given_name).toBe('Neema')
      expect(result.data.family_name).toBe('Mwakalinga')
    }
  })

  test('an optional field of spaces becomes empty, not a space', () => {
    const result = parse({ household_label: '  ', season_label: ' ' })
    expect(result.success).toBe(true)
    if (result.success) {
      expect(result.data.household_label).toBe('')
      expect(result.data.season_label).toBe('')
    }
  })
})

/**
 * The phone is the farmer's login name, and the farm is located by the
 * officer's GPS read (the fields fill and lock themselves; they open for
 * typing only when GPS fails). All three are required. Whether a phone is a
 * valid Tanzanian mobile is the database's call: `app_normalize_phone`.
 */
describe('phone and farm location are required', () => {
  test.each(['phone', 'farm_latitude', 'farm_longitude'] as const)('%s cannot be empty', (field) => {
    expect(issues(parse({ [field]: '' } as Partial<RegisterForm>))).toContain(
      `${field}:register.required`,
    )
  })

  test.each(['phone', 'farm_latitude', 'farm_longitude'] as const)('%s cannot be spaces', (field) => {
    expect(issues(parse({ [field]: '  ' } as Partial<RegisterForm>))).toContain(
      `${field}:register.required`,
    )
  })

})

describe('the crops grown on the plot', () => {
  test('at least one crop must be chosen', () => {
    expect(issues(parse({ cycles: [] }))).toEqual(['cycles:register.cropsRequired'])
  })

  test('several crops register together', () => {
    const coffee: CycleForm = { ...MAIZE_CYCLE, crop_id: COFFEE, area_ha: '', tree_count: '40' }
    expect(parse({ cycles: [MAIZE_CYCLE, coffee] }).success).toBe(true)
  })
})

/**
 * QA #19. The measure field is rendered conditionally on `crop.measured_by`,
 * and was the one field with no check — so the RPC's own prose
 * ("this crop is measured by area: area_ha is required") was doing the work of
 * an inline message, one round trip later. Each crop asks for its own.
 */
describe('the measure each crop is measured by', () => {
  test('area is required for an area crop', () => {
    expect(issues(parseCycle({ area_ha: '' }))).toContain('cycles.0.area_ha:register.required')
  })

  test('a tree count is required for a tree crop', () => {
    expect(issues(parseCycle({ crop_id: COFFEE, area_ha: '', tree_count: '' }))).toContain(
      'cycles.0.tree_count:register.required',
    )
  })

  test('a unit count is required for a unit crop', () => {
    expect(issues(parseCycle({ crop_id: HONEY, area_ha: '', unit_count: '' }))).toContain(
      'cycles.0.unit_count:register.required',
    )
  })

  // Only the matching measure is sent, so the others are irrelevant rather
  // than wrong. Demanding them would block a valid registration.
  test('the measures this crop does not use are not required', () => {
    expect(issues(parseCycle({ tree_count: '', unit_count: '' }))).toEqual([])
  })

  test('the second crop is checked as well as the first', () => {
    const coffee: CycleForm = { ...MAIZE_CYCLE, crop_id: COFFEE, area_ha: '', tree_count: '' }
    expect(issues(parse({ cycles: [MAIZE_CYCLE, coffee] }))).toEqual([
      'cycles.1.tree_count:register.required',
    ])
  })
})

/**
 * QA #21. `cycle_window_sane` caught this and its constraint NAME was rendered
 * as user copy. Two dates in one form need no knowledge of any database rule —
 * only that an end follows a start.
 */
describe('the harvest window', () => {
  test('an end before its start is caught on the end field', () => {
    expect(issues(parseCycle({ harvest_start: '2026-09-30', harvest_end: '2026-09-01' }))).toContain(
      'cycles.0.harvest_end:register.windowBackwards',
    )
  })

  // The constraint is `harvest_end >= harvest_start`. A single-day window is
  // legal and must stay legal.
  test('a window that starts and ends on the same day is fine', () => {
    expect(parseCycle({ harvest_start: '2026-09-01', harvest_end: '2026-09-01' }).success).toBe(true)
  })

  // The constraint reads `harvest_end is null or harvest_start is null or …`,
  // so one date alone is accepted by the database and must be here too.
  test('one date alone is not a backwards window', () => {
    expect(parseCycle({ harvest_start: '2026-09-01', harvest_end: '' }).success).toBe(true)
    expect(parseCycle({ harvest_start: '', harvest_end: '2026-09-30' }).success).toBe(true)
  })
})

/**
 * Bounding a field to its own column's type is not a copy of a business rule.
 * These are the shapes `numeric(10,4)`, `numeric(12,2)` and `numeric(9,6)`
 * will accept at all — the kind of failure that otherwise returns
 * "numeric field overflow", naming no field (QA #20).
 */
describe('numbers have to be numbers', () => {
  test('plot_area_ha rejects text', () => {
    expect(issues(parse({ plot_area_ha: 'abc' }))).toContain('plot_area_ha:register.notANumber')
  })

  test.each(['area_ha', 'harvest_quantity_kg'] as const)('a crop %s rejects text', (field) => {
    expect(issues(parseCycle({ [field]: 'abc' }))).toContain(`cycles.0.${field}:register.notANumber`)
  })

  test('a negative area is refused — the hectares domain checks value >= 0', () => {
    expect(issues(parse({ plot_area_ha: '-1' }))).toContain('plot_area_ha:register.notNegative')
  })

  test('an area past numeric(10,4) is refused before the database says "overflow"', () => {
    expect(issues(parse({ plot_area_ha: '1000000' }))).toContain('plot_area_ha:register.tooLarge')
  })

  test('a harvest past numeric(12,2) is refused the same way', () => {
    expect(issues(parseCycle({ harvest_quantity_kg: '999999999999' }))).toContain(
      'cycles.0.harvest_quantity_kg:register.tooLarge',
    )
  })

  test('a negative harvest is refused', () => {
    expect(issues(parseCycle({ harvest_quantity_kg: '-1' }))).toContain(
      'cycles.0.harvest_quantity_kg:register.notNegative',
    )
  })

  test('counts are whole things', () => {
    expect(issues(parseCycle({ crop_id: COFFEE, tree_count: '3.5' }))).toContain(
      'cycles.0.tree_count:register.wholeNumber',
    )
    expect(issues(parseCycle({ crop_id: HONEY, unit_count: '2.5' }))).toContain(
      'cycles.0.unit_count:register.wholeNumber',
    )
  })

  test('coordinates stay on the planet', () => {
    expect(issues(parse({ farm_latitude: '91' }))).toContain('farm_latitude:register.latitudeRange')
    expect(issues(parse({ farm_longitude: '181' }))).toContain(
      'farm_longitude:register.longitudeRange',
    )
    expect(parse({ farm_latitude: '-90', farm_longitude: '180' }).success).toBe(true)
  })

  test('a blank number is a blank, not a zero and not an error', () => {
    const result = parse({ plot_area_ha: '', cycles: [{ ...MAIZE_CYCLE, harvest_quantity_kg: '' }] })
    expect(issues(result)).toEqual([])
    if (result.success) expect(result.data.plot_area_ha).toBe('')
  })
})

/**
 * QA #27. `hectares` is `numeric(10,4)`, so 1.23456789 is stored as 1.2346 —
 * correctly, silently. The operator typed one number and the record holds
 * another.
 */
describe('roundedTo', () => {
  test('reports the value that will actually be stored', () => {
    expect(roundedTo('1.23456789', 4)).toBe('1.2346')
  })

  test('says nothing when nothing will change', () => {
    expect(roundedTo('1.8', 4)).toBeNull()
    expect(roundedTo('1.2346', 4)).toBeNull()
    expect(roundedTo('12', 4)).toBeNull()
  })

  test('says nothing about a blank or a non-number', () => {
    expect(roundedTo('', 4)).toBeNull()
    expect(roundedTo('abc', 4)).toBeNull()
    expect(roundedTo('   ', 4)).toBeNull()
  })

  // harvest_report.quantity_kg is numeric(12,2), a different scale on the
  // same screen.
  test('works at other scales', () => {
    expect(roundedTo('4100.567', 2)).toBe('4100.57')
    expect(roundedTo('4100.5', 2)).toBeNull()
  })

  // Trailing zeros past the scale change nothing, so warning about them would
  // be noise on a correct entry.
  test('trailing zeros are not a rounding', () => {
    expect(roundedTo('1.80000', 4)).toBeNull()
  })
})

/**
 * QA #22. A stored draft of the wrong shape was restored verbatim: First name
 * rendered as the literal `[object Object]`, Family name as `array`.
 *
 * This checks SHAPE, not validity. A half-filled draft is the entire point of
 * the feature, so an empty required field must still restore.
 */
describe('isRegisterDraft', () => {
  test('accepts a complete draft', () => {
    expect(isRegisterDraft(VALID)).toBe(true)
  })

  // The common case: the officer got two fields in before the phone died.
  test('accepts a half-filled one', () => {
    expect(isRegisterDraft({ ...VALID, family_name: '', cycles: [] })).toBe(true)
  })

  test('rejects the shapes #22 actually produced', () => {
    expect(isRegisterDraft({ ...VALID, given_name: { nested: true } })).toBe(false)
    expect(isRegisterDraft({ ...VALID, family_name: ['array'] })).toBe(false)
  })

  test('rejects a draft missing a field the form now has', () => {
    const { plot_label: _dropped, ...missing } = VALID
    expect(isRegisterDraft(missing)).toBe(false)
  })

  test('rejects a field whose type changed', () => {
    expect(isRegisterDraft({ ...VALID, is_head: 'yes' })).toBe(false)
    expect(isRegisterDraft({ ...VALID, cycles: [{ ...MAIZE_CYCLE, harvest_quantity_kg: 4100 }] })).toBe(false)
    expect(isRegisterDraft({ ...VALID, cycles: 'maize' })).toBe(false)
  })

  test('rejects a confidence outside the enum', () => {
    expect(isRegisterDraft({ ...VALID, confidence: 'certain' })).toBe(false)
  })

  test.each([null, undefined, 'a string', 42, []])('rejects %s', (value) => {
    expect(isRegisterDraft(value)).toBe(false)
  })

  test('tolerates a key the form no longer uses', () => {
    expect(isRegisterDraft({ ...VALID, removed_field: 'x' })).toBe(true)
  })
})

/**
 * A phone mid-registration when the app updated holds a draft of the ONE-crop
 * form. It is still the officer's work, so it restores, its crop becoming the
 * first of the list.
 */
describe('a draft written by the one-crop form', () => {
  const { cycles: _cycles, ...rest } = VALID
  const LEGACY = {
    ...rest,
    crop_id: MAIZE,
    cycle_area_ha: '1.6',
    cycle_tree_count: '',
    cycle_unit_count: '',
    harvest_start: '2026-09-01',
    harvest_end: '2026-09-30',
    harvest_quantity_kg: '4100',
  }

  test('is still accepted', () => {
    expect(isRegisterDraft(LEGACY)).toBe(true)
  })

  test('restores with its crop as the first of the list', () => {
    expect(toRegisterForm(LEGACY).cycles).toEqual([MAIZE_CYCLE])
    expect(toRegisterForm(LEGACY).plot_label).toBe(VALID.plot_label)
    expect(toRegisterForm(LEGACY)).not.toHaveProperty('crop_id')
  })

  test('with no crop chosen restores no crops', () => {
    expect(toRegisterForm({ ...LEGACY, crop_id: '' }).cycles).toEqual([])
  })

  test('a draft of the current form passes through unchanged', () => {
    expect(toRegisterForm(VALID)).toEqual(VALID)
  })
})
