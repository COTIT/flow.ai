import { describe, expect, test } from 'vitest'

import { validateRequestSearch } from '@/features/ops/requestSearch'

const VILLAGE = '30000000-0000-4000-8000-000000000001'

/**
 * Spec 7.2: "Filter state lives in the URL as validated search params — that
 * is why TanStack Router is in the stack."
 *
 * Validated means a hand-typed or stale URL degrades to the unfiltered view
 * rather than crashing the screen or sending nonsense to PostgREST.
 */
describe('validateRequestSearch', () => {
  test('an empty search is the unfiltered view', () => {
    expect(validateRequestSearch({})).toEqual({})
  })

  test('a real status is kept', () => {
    expect(validateRequestSearch({ status: 'submitted' })).toEqual({ status: 'submitted' })
    expect(validateRequestSearch({ status: 'under_review' })).toEqual({ status: 'under_review' })
  })

  test('a status that is not in the enum is dropped, not thrown', () => {
    expect(validateRequestSearch({ status: 'pending' })).toEqual({})
    expect(validateRequestSearch({ status: 'APPROVED' })).toEqual({})
    expect(validateRequestSearch({ status: 42 })).toEqual({})
  })

  test('a village id is kept when it looks like a uuid', () => {
    expect(validateRequestSearch({ village: VILLAGE })).toEqual({ village: VILLAGE })
  })

  // A non-uuid would reach PostgREST as an invalid input for a uuid column and
  // come back as an error, which is a worse answer than "no filter".
  test('a village that is not a uuid is dropped', () => {
    expect(validateRequestSearch({ village: 'ilundo' })).toEqual({})
    expect(validateRequestSearch({ village: '' })).toEqual({})
  })

  test('both filters together', () => {
    expect(validateRequestSearch({ status: 'approved', village: VILLAGE })).toEqual({
      status: 'approved',
      village: VILLAGE,
    })
  })

  test('unknown params are discarded rather than passed through', () => {
    expect(validateRequestSearch({ status: 'approved', sneaky: 'value' })).toEqual({
      status: 'approved',
    })
  })

  test('every status in the enum is accepted', () => {
    for (const status of [
      'draft',
      'submitted',
      'under_review',
      'approved',
      'rejected',
      'withdrawn',
    ]) {
      expect(validateRequestSearch({ status })).toEqual({ status })
    }
  })
})
