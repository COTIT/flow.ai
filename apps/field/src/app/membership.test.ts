import { describe, expect, test } from 'vitest'

import {
  activeMemberships,
  canAccessSurface,
  needsRoleChoice,
  resolveLanding,
  roleHome,
  safeRedirect,
  writableVillageIds,
  ownVillageId,
} from '@/app/membership'
import type { ActiveMembership } from '@/app/membership'

const m = (
  role: ActiveMembership['role'],
  extra: Partial<ActiveMembership> = {},
): ActiveMembership => ({
  id: crypto.randomUUID(),
  role,
  project_id: '20000000-0000-4000-8000-000000000001',
  village_id: null,
  revoked_at: null,
  ...extra,
})

describe('roleHome', () => {
  test('each product role has its own surface', () => {
    expect(roleHome('farmer')).toBe('/farm')
    expect(roleHome('field_officer')).toBe('/officer')
    expect(roleHome('ops')).toBe('/ops')
  })

  // CLAUDE.md: admin is operational (seed, support), not a product surface.
  // There is no /admin route, and admin's scope in the role matrix is the
  // whole project, identical to ops.
  test('admin lands on the ops surface, since no admin surface exists', () => {
    expect(roleHome('admin')).toBe('/ops')
  })
})

describe('activeMemberships', () => {
  test('drops revoked rows', () => {
    const rows = [m('farmer'), m('ops', { revoked_at: '2026-09-01T00:00:00Z' })]
    expect(activeMemberships(rows)).toHaveLength(1)
    expect(activeMemberships(rows)[0].role).toBe('farmer')
  })

  test('a fully revoked user has no active memberships', () => {
    expect(activeMemberships([m('ops', { revoked_at: '2026-09-01T00:00:00Z' })])).toEqual([])
  })
})

describe('resolveLanding', () => {
  // Spec 4.2: 0 rows -> /no-access, 1 row -> that role's home,
  // 2+ rows -> /select-role.
  test('zero memberships is a real screen, not a crash', () => {
    expect(resolveLanding([])).toEqual({ to: '/no-access' })
  })

  test('one membership goes straight to that role home', () => {
    expect(resolveLanding([m('farmer')])).toEqual({ to: '/farm' })
    expect(resolveLanding([m('field_officer')])).toEqual({ to: '/officer' })
    expect(resolveLanding([m('ops')])).toEqual({ to: '/ops' })
  })

  test('two or more memberships need the picker', () => {
    expect(resolveLanding([m('farmer'), m('field_officer')])).toEqual({ to: '/select-role' })
  })

  test('revoked rows do not count toward the picker', () => {
    const rows = [m('farmer'), m('ops', { revoked_at: '2026-09-01T00:00:00Z' })]
    expect(resolveLanding(rows)).toEqual({ to: '/farm' })
  })

  test('a user whose only membership was revoked gets no-access', () => {
    expect(resolveLanding([m('ops', { revoked_at: '2026-09-01T00:00:00Z' })])).toEqual({
      to: '/no-access',
    })
  })
})

describe('canAccessSurface', () => {
  // Spec 3: beforeLoad redirects when the role does not match. Spec 13: the
  // guard is UX, not security — RLS still returns zero rows if someone gets
  // through. This function only answers the UX question.
  test('a farmer may only enter the farmer surface', () => {
    const rows = [m('farmer')]
    expect(canAccessSurface(rows, 'farmer')).toBe(true)
    expect(canAccessSurface(rows, 'officer')).toBe(false)
    expect(canAccessSurface(rows, 'ops')).toBe(false)
  })

  test('an officer reaches the officer surface but not the others', () => {
    const rows = [m('field_officer')]
    expect(canAccessSurface(rows, 'officer')).toBe(true)
    expect(canAccessSurface(rows, 'farmer')).toBe(false)
    expect(canAccessSurface(rows, 'ops')).toBe(false)
  })

  test('ops and admin both reach the ops surface, Tower included', () => {
    expect(canAccessSurface([m('ops')], 'ops')).toBe(true)
    expect(canAccessSurface([m('admin')], 'ops')).toBe(true)
  })

  // The role matrix gives ops and admin read + write on person, farm, plot and
  // crop_cycle, and those records live on the officer surface. Blocking them
  // also breaks the Tower's traceability claim: the opportunity screen drills
  // to /officer/people/$personId, and spec 8.2 requires ops to reach a
  // farmer's record from any Tower headline in at most three clicks.
  test('ops and admin reach the officer surface, because the records live there', () => {
    expect(canAccessSurface([m('ops')], 'officer')).toBe(true)
    expect(canAccessSurface([m('admin')], 'officer')).toBe(true)
  })

  // Not symmetric: the farmer surface is scoped by app_farms(), which resolves
  // through a person. Ops holds no person_id, so /farm/my-farm would render an
  // empty state for them — a redirect is the more honest answer.
  test('ops does not enter the farmer surface', () => {
    expect(canAccessSurface([m('ops')], 'farmer')).toBe(false)
    expect(canAccessSurface([m('admin')], 'farmer')).toBe(false)
  })

  test('holding two roles opens both surfaces', () => {
    const rows = [m('farmer'), m('field_officer')]
    expect(canAccessSurface(rows, 'farmer')).toBe(true)
    expect(canAccessSurface(rows, 'officer')).toBe(true)
    expect(canAccessSurface(rows, 'ops')).toBe(false)
  })

  test('a revoked ops role closes the officer surface too', () => {
    const rows = [m('ops', { revoked_at: '2026-09-01T00:00:00Z' })]
    expect(canAccessSurface(rows, 'officer')).toBe(false)
    expect(canAccessSurface(rows, 'ops')).toBe(false)
  })

  test('a revoked role closes its surface on the next evaluation', () => {
    const rows = [m('ops', { revoked_at: '2026-09-01T00:00:00Z' })]
    expect(canAccessSurface(rows, 'ops')).toBe(false)
  })
})

describe('safeRedirect', () => {
  test('keeps an in-app path', () => {
    expect(safeRedirect('/ops/tower')).toBe('/ops/tower')
    expect(safeRedirect('/farm')).toBe('/farm')
  })

  test('keeps a path with a query string', () => {
    expect(safeRedirect('/ops/requests?status=submitted')).toBe('/ops/requests?status=submitted')
  })

  // The value arrives from the URL, so it is attacker-controllable. Anything
  // that could leave the origin is discarded rather than sanitised.
  test('discards anything that could leave the origin', () => {
    expect(safeRedirect('//evil.example.com')).toBeUndefined()
    expect(safeRedirect('https://evil.example.com')).toBeUndefined()
    expect(safeRedirect('http://evil.example.com')).toBeUndefined()
    expect(safeRedirect('javascript:alert(1)')).toBeUndefined()
    expect(safeRedirect('/\\evil.example.com')).toBeUndefined()
  })

  test('discards relative and empty values', () => {
    expect(safeRedirect('ops')).toBeUndefined()
    expect(safeRedirect('')).toBeUndefined()
    expect(safeRedirect(undefined)).toBeUndefined()
  })

  test('does not send anyone back to the login screen', () => {
    expect(safeRedirect('/login')).toBeUndefined()
  })
})

describe('writableVillageIds', () => {
  const ILUNDO = '30000000-0000-4000-8000-000000000001'
  const MGAMA = '30000000-0000-4000-8000-000000000002'

  test('an officer may write into the village they are assigned', () => {
    expect(writableVillageIds([m('field_officer', { village_id: ILUNDO })])).toEqual([ILUNDO])
  })

  test('two assignments give two villages', () => {
    const rows = [
      m('field_officer', { village_id: ILUNDO }),
      m('field_officer', { village_id: MGAMA }),
    ]
    expect(writableVillageIds(rows).sort()).toEqual([ILUNDO, MGAMA].sort())
  })

  // village_id NULL is whole-project scope. There is no single village to
  // register into, so ops must choose one rather than have one inferred.
  test('a project-wide membership yields no specific village', () => {
    expect(writableVillageIds([m('ops', { village_id: null })])).toEqual([])
  })

  test('a farmer village is not writable', () => {
    expect(writableVillageIds([m('farmer', { village_id: ILUNDO })])).toEqual([])
  })

  test('revoked assignments do not grant a village', () => {
    expect(
      writableVillageIds([
        m('field_officer', { village_id: ILUNDO, revoked_at: '2026-09-01T00:00:00Z' }),
      ]),
    ).toEqual([])
  })

  test('duplicates collapse', () => {
    const rows = [
      m('field_officer', { village_id: ILUNDO }),
      m('field_officer', { village_id: ILUNDO }),
    ]
    expect(writableVillageIds(rows)).toEqual([ILUNDO])
  })
})

describe('ownVillageId', () => {
  const ILUNDO = '30000000-0000-4000-8000-000000000001'

  test('a farmer village comes from their membership', () => {
    expect(ownVillageId([m('farmer', { village_id: ILUNDO })])).toBe(ILUNDO)
  })

  // village_id NULL is whole-project scope: there is no one village.
  test('a project-wide membership has no single village', () => {
    expect(ownVillageId([m('ops', { village_id: null })])).toBeUndefined()
  })

  test('revoked memberships do not supply a village', () => {
    expect(
      ownVillageId([m('farmer', { village_id: ILUNDO, revoked_at: '2026-09-01T00:00:00Z' })]),
    ).toBeUndefined()
  })

  test('no memberships at all yields nothing', () => {
    expect(ownVillageId([])).toBeUndefined()
  })
})

/**
 * QA #10. `/select-role` told the Ilundo officer — who holds exactly one
 * membership — "You hold more than one role. Pick the one you want to work
 * in", above a single option. The copy asserts something untrue, and
 * `resolveLanding` already knows better: nothing ROUTES there for a
 * single-role user, only a typed URL reaches it.
 */
describe('needsRoleChoice', () => {
  test('nobody with one role has a choice to make', () => {
    expect(needsRoleChoice([m('field_officer')])).toBe(false)
  })

  test('two roles is a real choice', () => {
    expect(needsRoleChoice([m('field_officer'), m('ops')])).toBe(true)
  })

  test('no memberships is not a choice either', () => {
    expect(needsRoleChoice([])).toBe(false)
  })

  // Revoked rows are not roles. Two memberships, one revoked, is one role.
  test('a revoked membership does not create a choice', () => {
    expect(
      needsRoleChoice([m('field_officer'), { ...m('ops'), revoked_at: '2026-01-01T00:00:00Z' }]),
    ).toBe(false)
  })

  // Two villages on the SAME role is not a role choice — it is one role.
  test('the same role twice is still one role', () => {
    expect(needsRoleChoice([m('field_officer'), m('field_officer')])).toBe(false)
  })
})
