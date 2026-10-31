import type { Database } from '@/lib/db.types'

export type RequestStatus = Database['public']['Enums']['pue_status']

/**
 * pue_request_guard's machine, mirrored (business-rules §2).
 *
 * This is NOT a client-side copy of the rule: the trigger remains the
 * enforcement, and an "illegal transition" error reaching a user means this
 * UI offered a control it should not have. What this table does is decide
 * which controls to render at all.
 */
export const LEGAL_TRANSITIONS: Record<RequestStatus, RequestStatus[]> = {
  draft: ['submitted', 'withdrawn'],
  submitted: ['under_review', 'withdrawn'],
  under_review: ['approved', 'rejected'],
  // Terminal.
  approved: [],
  rejected: [],
  withdrawn: [],
}

export function isLegalTransition(from: RequestStatus, to: RequestStatus): boolean {
  return LEGAL_TRANSITIONS[from].includes(to)
}

/** Spec 7.3 names exactly three: Start review, Approve, Reject. */
export type ReviewerAction = 'start_review' | 'approve' | 'reject'

export const ACTION_TARGET: Record<ReviewerAction, RequestStatus> = {
  start_review: 'under_review',
  approve: 'approved',
  reject: 'rejected',
}

/**
 * What ops may do to a request from the review screen.
 *
 * A draft offers nothing here: it belongs to the applicant, and reviewing
 * something that has not been submitted would be reviewing a request nobody
 * has made.
 */
export function reviewerActions(status: RequestStatus): ReviewerAction[] {
  switch (status) {
    case 'submitted':
      return ['start_review']
    case 'under_review':
      return ['approve', 'reject']
    default:
      return []
  }
}

/** Spec 7.3: approve and reject both require a decision_note. */
export function requiresDecisionNote(action: ReviewerAction): boolean {
  return action === 'approve' || action === 'reject'
}

/**
 * What the APPLICANT may do to their own request — business-rules §2's role
 * matrix, farmer column.
 *
 * Lives beside `reviewerActions` because they are two halves of one machine
 * and must not drift: a status is either the applicant's to act on or the
 * reviewer's, never both. `draft` belongs to the applicant alone, and
 * `under_review` to ops alone.
 *
 * Editing a draft is also permitted by the matrix, but is deferred with the
 * other officer and farmer write surfaces — this covers the two transitions.
 */
export type FarmerAction = 'submit' | 'withdraw'

export const FARMER_ACTION_TARGET: Record<FarmerAction, RequestStatus> = {
  submit: 'submitted',
  withdraw: 'withdrawn',
}

export function farmerActions(status: RequestStatus): FarmerAction[] {
  switch (status) {
    case 'draft':
      return ['submit', 'withdraw']
    case 'submitted':
      // Withdraw only. Starting a review is the reviewer's move.
      return ['withdraw']
    default:
      return []
  }
}
