import { describe, expect, test } from 'vitest'

import {
  FARMER_ACTION_TARGET,
  LEGAL_TRANSITIONS,
  farmerActions,
  isLegalTransition,
  requiresDecisionNote,
  reviewerActions,
  type RequestStatus,
} from '@/features/ops/transitions'

/**
 * Mirrors pue_request_guard's machine (business-rules §2):
 *   draft        -> submitted | withdrawn
 *   submitted    -> under_review | withdrawn
 *   under_review -> approved | rejected
 * approved, rejected and withdrawn are terminal.
 *
 * The UI offers only legal transitions and surfaces the trigger's message
 * verbatim if one is raised anyway. This is not a client-side copy of the
 * rule — the trigger remains the enforcement — it decides which controls to
 * render, and an "illegal transition" error means the UI offered something it
 * should not have.
 */
describe('LEGAL_TRANSITIONS', () => {
  test('matches the trigger exactly', () => {
    expect(LEGAL_TRANSITIONS).toEqual({
      draft: ['submitted', 'withdrawn'],
      submitted: ['under_review', 'withdrawn'],
      under_review: ['approved', 'rejected'],
      approved: [],
      rejected: [],
      withdrawn: [],
    })
  })

  test('approved, rejected and withdrawn are terminal', () => {
    expect(LEGAL_TRANSITIONS.approved).toEqual([])
    expect(LEGAL_TRANSITIONS.rejected).toEqual([])
    expect(LEGAL_TRANSITIONS.withdrawn).toEqual([])
  })

  test('a request cannot jump straight from draft to approved', () => {
    expect(isLegalTransition('draft', 'approved')).toBe(false)
    expect(isLegalTransition('draft', 'under_review')).toBe(false)
  })

  test('the legal path through review is allowed at each step', () => {
    expect(isLegalTransition('draft', 'submitted')).toBe(true)
    expect(isLegalTransition('submitted', 'under_review')).toBe(true)
    expect(isLegalTransition('under_review', 'approved')).toBe(true)
    expect(isLegalTransition('under_review', 'rejected')).toBe(true)
  })

  test('a decided request cannot be reopened', () => {
    expect(isLegalTransition('approved', 'under_review')).toBe(false)
    expect(isLegalTransition('rejected', 'submitted')).toBe(false)
    expect(isLegalTransition('withdrawn', 'submitted')).toBe(false)
  })
})

describe('reviewerActions', () => {
  // Spec 7.3 names exactly three actions: Start review, Approve, Reject.
  test('a submitted request can only be taken into review', () => {
    expect(reviewerActions('submitted')).toEqual(['start_review'])
  })

  test('a request under review can be approved or rejected', () => {
    expect(reviewerActions('under_review')).toEqual(['approve', 'reject'])
  })

  // A draft belongs to the applicant. Ops reviewing it before it is submitted
  // would be reviewing something nobody has asked for.
  test('a draft offers ops nothing on the review screen', () => {
    expect(reviewerActions('draft')).toEqual([])
  })

  test('terminal states offer nothing', () => {
    expect(reviewerActions('approved')).toEqual([])
    expect(reviewerActions('rejected')).toEqual([])
    expect(reviewerActions('withdrawn')).toEqual([])
  })
})

describe('requiresDecisionNote', () => {
  // Spec 7.3: "Approve, Reject — both requiring a decision_note."
  test('a decision must be explained', () => {
    expect(requiresDecisionNote('approve')).toBe(true)
    expect(requiresDecisionNote('reject')).toBe(true)
  })

  test('starting a review is not a decision and needs no note', () => {
    expect(requiresDecisionNote('start_review')).toBe(false)
  })
})

/**
 * The farmer half of business-rules §2's role matrix, which the ops half
 * above already mirrors:
 *
 *   draft        farmer may edit, submit, withdraw
 *   submitted    withdraw only
 *   under_review nothing — it is with ops
 *   terminal     nothing
 *
 * QA-FINDINGS #5: the draft detail screen offered no control at all, so
 * `draft -> submitted` was a transition the database permits and the UI could
 * not reach. The seeded cold-room draft was inert.
 */
describe('farmerActions', () => {
  test('a draft can be submitted or withdrawn', () => {
    expect(farmerActions('draft')).toEqual(['submit', 'withdraw'])
  })

  test('a submitted request can only be withdrawn', () => {
    expect(farmerActions('submitted')).toEqual(['withdraw'])
  })

  // Under review belongs to ops. A farmer withdrawing mid-review would pull
  // the request out from under a reviewer, and the guard refuses it.
  test('a request under review offers the farmer nothing', () => {
    expect(farmerActions('under_review')).toEqual([])
  })

  test('terminal states offer nothing', () => {
    expect(farmerActions('approved')).toEqual([])
    expect(farmerActions('rejected')).toEqual([])
    expect(farmerActions('withdrawn')).toEqual([])
  })

  // The UI must only ever offer transitions the trigger will accept: an
  // "illegal transition" error reaching a farmer means this table is wrong.
  test('every action it offers is a legal transition', () => {
    const statuses: RequestStatus[] = [
      'draft',
      'submitted',
      'under_review',
      'approved',
      'rejected',
      'withdrawn',
    ]
    for (const status of statuses) {
      for (const action of farmerActions(status)) {
        expect(isLegalTransition(status, FARMER_ACTION_TARGET[action])).toBe(true)
      }
    }
  })

  test('the farmer and ops action sets never overlap', () => {
    // draft is the applicant's alone; under_review is the reviewer's alone.
    expect(reviewerActions('draft')).toEqual([])
    expect(farmerActions('under_review')).toEqual([])
  })
})
