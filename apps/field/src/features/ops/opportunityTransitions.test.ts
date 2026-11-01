import { describe, expect, test } from 'vitest'

import {
  LEGAL_OPPORTUNITY_TRANSITIONS,
  OPPORTUNITY_ACTION_TARGET,
  RELEASES_SUPPLY,
  isLegalOpportunityTransition,
  opportunityActions,
  releasesSupply,
  type OpportunityAction,
  type OpportunityStatus,
} from '@/features/ops/opportunityTransitions'

const ALL: OpportunityStatus[] = ['proposed', 'shared', 'accepted', 'declined', 'lapsed']

describe('the opportunity status machine', () => {
  // The five values in opportunity_status, no more and no less. A sixth here
  // would mean someone invented an enum value; a missing one would mean a
  // status the UI can never render a control for.
  test('covers every status in the enum', () => {
    expect(Object.keys(LEGAL_OPPORTUNITY_TRANSITIONS).sort()).toEqual([...ALL].sort())
  })

  test('a proposed opportunity can be shared', () => {
    expect(isLegalOpportunityTransition('proposed', 'shared')).toBe(true)
  })

  // business-rules §8: accepted means both sides agreed to keep talking. That
  // only follows a buyer having seen it.
  test('accepted follows shared, never proposed directly', () => {
    expect(isLegalOpportunityTransition('shared', 'accepted')).toBe(true)
    expect(isLegalOpportunityTransition('proposed', 'accepted')).toBe(false)
  })

  test('any live status can be declined or lapsed', () => {
    for (const from of ['proposed', 'shared', 'accepted'] as const) {
      expect(isLegalOpportunityTransition(from, 'declined')).toBe(true)
      expect(isLegalOpportunityTransition(from, 'lapsed')).toBe(true)
    }
  })

  // Nothing may leave declined or lapsed. This is the machine's one load-
  // bearing rule and it is NOT cosmetic: opportunity_supply_guard runs on
  // supply writes only, so re-opening a released opportunity would re-commit
  // its lines with no guard between — past whatever else was committed in the
  // meantime. The release is one-way because the check that would make it
  // safe does not exist.
  test('declined and lapsed are terminal', () => {
    expect(LEGAL_OPPORTUNITY_TRANSITIONS.declined).toEqual([])
    expect(LEGAL_OPPORTUNITY_TRANSITIONS.lapsed).toEqual([])
    for (const to of ALL) {
      expect(isLegalOpportunityTransition('declined', to)).toBe(false)
      expect(isLegalOpportunityTransition('lapsed', to)).toBe(false)
    }
  })

  test('no status transitions to itself', () => {
    for (const from of ALL) {
      expect(LEGAL_OPPORTUNITY_TRANSITIONS[from]).not.toContain(from)
    }
  })

  test('every legal target is itself a real status', () => {
    for (const targets of Object.values(LEGAL_OPPORTUNITY_TRANSITIONS)) {
      for (const to of targets) expect(ALL).toContain(to)
    }
  })
})

describe('which controls a screen offers', () => {
  test('a proposed opportunity offers share, decline and lapse', () => {
    expect(opportunityActions('proposed')).toEqual(['share', 'decline', 'lapse'])
  })

  test('a shared opportunity offers accept, decline and lapse', () => {
    expect(opportunityActions('shared')).toEqual(['accept', 'decline', 'lapse'])
  })

  // Accepted is not the end: a buyer can still walk away, and a window still
  // passes. Both release the supply.
  test('an accepted opportunity can still be declined or lapsed', () => {
    expect(opportunityActions('accepted')).toEqual(['decline', 'lapse'])
  })

  test('a released opportunity offers nothing', () => {
    expect(opportunityActions('declined')).toEqual([])
    expect(opportunityActions('lapsed')).toEqual([])
  })

  test('every offered action targets a legal transition from that status', () => {
    for (const from of ALL) {
      for (const action of opportunityActions(from)) {
        expect(isLegalOpportunityTransition(from, OPPORTUNITY_ACTION_TARGET[action])).toBe(true)
      }
    }
  })

  // The inverse of the test above: the machine and the controls must be two
  // views of one thing, so a legal transition nobody can perform is a bug.
  test('every legal transition is reachable through some action', () => {
    for (const from of ALL) {
      const reachable = opportunityActions(from).map((a) => OPPORTUNITY_ACTION_TARGET[a])
      expect([...reachable].sort()).toEqual([...LEGAL_OPPORTUNITY_TRANSITIONS[from]].sort())
    }
  })
})

describe('which transitions release committed supply', () => {
  // business-rules §7: committed_kg sums supply on opportunities in
  // proposed/shared/accepted. Declining or lapsing drops the opportunity out
  // of that set, which IS the release — there is no detach and no delete.
  test('declined and lapsed release, the live statuses do not', () => {
    expect([...RELEASES_SUPPLY].sort()).toEqual(['declined', 'lapsed'])
    expect(releasesSupply('decline')).toBe(true)
    expect(releasesSupply('lapse')).toBe(true)
    expect(releasesSupply('share')).toBe(false)
    expect(releasesSupply('accept')).toBe(false)
  })

  // The set is read straight off v_harvest_available's own filter clause, so
  // a change to one must break the other.
  test('the releasing statuses are exactly those outside the committed set', () => {
    const committed: OpportunityStatus[] = ['proposed', 'shared', 'accepted']
    expect([...ALL].filter((s) => !committed.includes(s)).sort()).toEqual([...RELEASES_SUPPLY].sort())
  })

  test('an action that releases is one a user should be warned about', () => {
    const actions: OpportunityAction[] = ['share', 'accept', 'decline', 'lapse']
    expect(actions.filter(releasesSupply)).toEqual(['decline', 'lapse'])
  })
})
