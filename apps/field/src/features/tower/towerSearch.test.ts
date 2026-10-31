import { describe, expect, test } from 'vitest'

import { validateQualitySearch } from '@/features/tower/towerSearch'

const VILLAGE = '20000000-0000-4000-8000-000000000001'

describe('quality drill URL filters', () => {
  test('keeps valid village and metric', () => {
    expect(validateQualitySearch({ village: VILLAGE, metric: 'farms' })).toEqual({ village: VILLAGE, metric: 'farms' })
  })

  test('drops invalid village and metric before queries', () => {
    expect(validateQualitySearch({ village: 'not-a-uuid', metric: 'invented' })).toEqual({ metric: 'persons' })
  })
})
