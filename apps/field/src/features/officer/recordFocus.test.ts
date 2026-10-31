import { describe, expect, test } from 'vitest'

import { validateCycleFocus, validateFarmFocus } from '@/features/officer/recordFocus'

describe('detail focus search validation', () => {
  test('accepts UUID focus values', () => {
    const id = '11111111-1111-4111-8111-111111111111'
    expect(validateFarmFocus({ plot: id })).toEqual({ plot: id })
    expect(validateCycleFocus({ harvest: id })).toEqual({ harvest: id })
  })

  test('drops missing, wrong-type, and malformed values', () => {
    expect(validateFarmFocus({ plot: 'nope' })).toEqual({})
    expect(validateFarmFocus({ plot: 4 })).toEqual({})
    expect(validateCycleFocus({ harvest: null })).toEqual({})
    expect(validateCycleFocus({})).toEqual({})
  })
})
