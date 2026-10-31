import type { Database } from '@/lib/db.types'

export type OpportunityStatus = Database['public']['Enums']['opportunity_status']

/**
 * The opportunity status machine.
 *
 * Unlike `LEGAL_TRANSITIONS` for requests, this table has NO trigger behind
 * it. `pue_request_guard` enforces the request machine and the client table
 * only decides which controls to draw; `opportunity_update` is a plain policy
 * — staff may set any status on any opportunity in their villages. So this
 * file is the only thing holding the order, and it is worth saying why each
 * edge is here rather than treating the shape as obvious.
 *
 * The transitions come from business-rules §7 and §8:
 *
 *   proposed   ops recorded that a village could supply a buyer
 *   shared     it has been put in front of the buyer
 *   accepted   both sides agreed to KEEP TALKING (§8) — not a sale
 *   declined   the buyer said no
 *   lapsed     the window passed with no answer
 *
 * `accepted` cannot follow `proposed` directly because accepted describes the
 * buyer's response, and a buyer who has not been shown the opportunity has not
 * responded to it.
 *
 * **Declined and lapsed are terminal, and that is load-bearing.** They are the
 * two statuses outside `v_harvest_available`'s committed set, so entering
 * either releases the supply (§7). Re-opening one would re-commit those lines
 * — and `opportunity_supply_guard` fires on supply writes only, never on a
 * status change, so nothing would check the released kg against whatever was
 * committed elsewhere in the meantime. The release is one-way because the
 * check that would make it reversible does not exist.
 */
export const LEGAL_OPPORTUNITY_TRANSITIONS: Record<OpportunityStatus, OpportunityStatus[]> = {
  proposed: ['shared', 'declined', 'lapsed'],
  shared: ['accepted', 'declined', 'lapsed'],
  accepted: ['declined', 'lapsed'],
  declined: [],
  lapsed: [],
}

export function isLegalOpportunityTransition(
  from: OpportunityStatus,
  to: OpportunityStatus,
): boolean {
  return LEGAL_OPPORTUNITY_TRANSITIONS[from].includes(to)
}

export type OpportunityAction = 'share' | 'accept' | 'decline' | 'lapse'

export const OPPORTUNITY_ACTION_TARGET: Record<OpportunityAction, OpportunityStatus> = {
  share: 'shared',
  accept: 'accepted',
  decline: 'declined',
  lapse: 'lapsed',
}

/**
 * What ops may do to an opportunity from the detail screen.
 *
 * Ordered forward-first: the one step along the happy path, then the two that
 * end it. Declining and lapsing stay available from every live status because
 * a buyer can walk away at any point and a window passes on its own.
 */
export function opportunityActions(status: OpportunityStatus): OpportunityAction[] {
  switch (status) {
    case 'proposed':
      return ['share', 'decline', 'lapse']
    case 'shared':
      return ['accept', 'decline', 'lapse']
    case 'accepted':
      return ['decline', 'lapse']
    default:
      return []
  }
}

/**
 * The statuses OUTSIDE `v_harvest_available`'s committed set.
 *
 * Copied from that view's own filter clause — `o.status in ('proposed',
 * 'shared','accepted')` — as its complement, so that the pair cannot drift
 * without a test failing.
 */
export const RELEASES_SUPPLY: readonly OpportunityStatus[] = ['declined', 'lapsed']

/** Whether performing this action returns committed kg to available supply. */
export function releasesSupply(action: OpportunityAction): boolean {
  return RELEASES_SUPPLY.includes(OPPORTUNITY_ACTION_TARGET[action])
}
