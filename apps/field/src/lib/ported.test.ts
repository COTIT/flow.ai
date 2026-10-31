import { describe, expect, test } from 'vitest'

import { isPorted } from '@/lib/ported'

describe('isPorted', () => {
  const list = ['/officer/register', '/farm/surveys/*']

  test('exact paths match, trailing slash ignored', () => {
    expect(isPorted('/officer/register', list)).toBe(true)
    expect(isPorted('/officer/register/', list)).toBe(true)
    expect(isPorted('/officer', list)).toBe(false)
  })

  test('a prefix entry covers everything below it', () => {
    expect(isPorted('/farm/surveys/abc', list)).toBe(true)
    expect(isPorted('/farm/surveyswhatever', list)).toBe(false)
  })
})
