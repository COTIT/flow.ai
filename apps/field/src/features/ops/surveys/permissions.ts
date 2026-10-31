import { activeMemberships, type ActiveMembership } from '@/app/membership'

/**
 * Whether to RENDER survey authoring controls.
 *
 * Authoring is admin-only, and survey_guard plus the survey insert/update
 * policies enforce that — RLS is the boundary. This only decides whether the
 * controls appear: ops read the same lists and results, and may void a
 * voucher, but a draft editor on their screen would offer writes the database
 * refuses.
 */
export function canAuthorSurveys(memberships: ActiveMembership[] | undefined): boolean {
  return activeMemberships(memberships ?? []).some((m) => m.role === 'admin')
}

/** The project a new survey belongs to: the active admin membership's. */
export function authoringProjectId(memberships: ActiveMembership[] | undefined): string | undefined {
  return activeMemberships(memberships ?? []).find((m) => m.role === 'admin')?.project_id
}
