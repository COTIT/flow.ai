import { activeMemberships, type ActiveMembership } from '@/app/membership'

/**
 * Whether to RENDER survey authoring controls.
 *
 * Authoring is admin-only, and the API enforces that (403) — it is the
 * boundary. This only decides whether the
 * controls appear: ops read the same lists and results, and may void a
 * voucher, but a draft editor on their screen would offer writes the API
 * refuses.
 */
export function canAuthorSurveys(memberships: ActiveMembership[] | undefined): boolean {
  return activeMemberships(memberships ?? []).some((m) => m.role === 'admin')
}
