import type { Database } from '@/lib/db.types'

type MembershipRow = Database['public']['Tables']['membership']['Row']
export type AppRole = Database['public']['Enums']['app_role']

/** The subset of a membership row the routing decision needs. */
export type ActiveMembership = Pick<
  MembershipRow,
  'id' | 'role' | 'project_id' | 'village_id' | 'revoked_at'
>

/**
 * Where a role starts.
 *
 * `admin` has no surface of its own: CLAUDE.md calls it operational (seed,
 * support), the route map has no /admin, and the role matrix gives it the same
 * whole-project scope as ops. So it lands on the ops surface.
 */
export function roleHome(role: AppRole): string {
  switch (role) {
    case 'farmer':
      return '/farm'
    case 'field_officer':
      return '/officer'
    case 'ops':
    case 'admin':
      return '/ops'
  }
}

/**
 * Access ends, the audit trail does not — membership is revoked, never
 * deleted, so every read has to filter.
 */
export function activeMemberships(rows: ActiveMembership[]): ActiveMembership[] {
  return rows.filter((r) => r.revoked_at === null)
}

/**
 * Spec 4.2: 0 rows -> /no-access, 1 row -> that role's home, 2+ -> the picker.
 *
 * Route guards are UX, not security. This decides where to *send* someone;
 * what they can see once there is RLS's business.
 */
export function resolveLanding(rows: ActiveMembership[]): { to: string } {
  const active = activeMemberships(rows)
  if (active.length === 0) return { to: '/no-access' }
  if (active.length === 1) return { to: roleHome(active[0].role) }
  return { to: '/select-role' }
}

/** The three layout route groups. The Tower lives inside `ops`. */
export type Surface = 'farmer' | 'officer' | 'ops'

const SURFACE_ROLES: Record<Surface, AppRole[]> = {
  // Scoped by app_farms(), which resolves through a person. Ops holds no
  // person_id, so this surface would render empty for them — a redirect is
  // the more honest answer than a page of nothing.
  farmer: ['farmer'],
  // Ops and admin belong here too. The role matrix gives them read + write on
  // person, farm, plot and crop_cycle, and those records live on this surface.
  // It is also what makes the Tower's traceability claim work: spec 8.2
  // requires reaching a farmer's record from any headline in three clicks, and
  // the opportunity screen drills to /officer/people/$personId.
  officer: ['field_officer', 'ops', 'admin'],
  // admin is operational and shares the ops surface — see roleHome.
  ops: ['ops', 'admin'],
}

/**
 * Whether a role held by this user opens this surface.
 *
 * This answers a UX question only. RLS is the security boundary: someone who
 * reaches a surface anyway gets the page shell and zero rows, which is correct
 * behaviour rather than a hole.
 */
/**
 * Does this user have a role choice to make at all? — QA #10.
 *
 * `/select-role` claimed "You hold more than one role" above a single option
 * for the Ilundo officer. Nothing routes there for a single-role user —
 * `resolveLanding` sends them straight home — so only a typed URL arrives, and
 * the screen should send them home rather than assert something untrue.
 *
 * Counted by DISTINCT ROLE, not by membership row: an officer assigned to two
 * villages holds one role and has nothing to choose between.
 */
export function needsRoleChoice(rows: ActiveMembership[]): boolean {
  return new Set(activeMemberships(rows).map((m) => m.role)).size > 1
}

export function canAccessSurface(rows: ActiveMembership[], surface: Surface): boolean {
  const allowed = SURFACE_ROLES[surface]
  return activeMemberships(rows).some((m) => allowed.includes(m.role))
}

/**
 * Validates the `redirect` search param the surface guards attach when they
 * bounce an unauthenticated visitor.
 *
 * The value comes from the URL and is therefore attacker-controllable, so
 * anything that could leave this origin is discarded rather than patched up:
 * a scheme, a protocol-relative `//host`, or a backslash Windows-style host.
 * Returns undefined when the value cannot be trusted, and the caller falls
 * back to the role's own home.
 */
export function safeRedirect(target: string | undefined): string | undefined {
  if (!target) return undefined
  if (!target.startsWith('/')) return undefined
  if (target.startsWith('//') || target.startsWith('/\\')) return undefined
  // Sending someone back to /login after signing in would loop.
  if (target === '/login' || target.startsWith('/login?')) return undefined
  return target
}

/**
 * Villages a field officer is assigned to, and may therefore register into.
 *
 * `village_id` NULL is whole-project scope, which is ops and admin — there is
 * no single village to infer, so those yield nothing and the surface has to
 * ask. This mirrors app_villages() but answers a narrower question: not "what
 * may I read" but "where may I create a record".
 */
export function writableVillageIds(rows: ActiveMembership[]): string[] {
  const ids = new Set<string>()
  for (const m of activeMemberships(rows)) {
    if (m.role !== 'field_officer') continue
    if (m.village_id) ids.add(m.village_id)
  }
  return [...ids]
}

/**
 * The single village this session belongs to, if there is one.
 *
 * A farmer's membership names their village; ops and admin hold whole-project
 * scope with village_id NULL, so there is no one village to return and the
 * caller must ask rather than guess.
 */
export function ownVillageId(rows: ActiveMembership[]): string | undefined {
  for (const m of activeMemberships(rows)) {
    if (m.village_id) return m.village_id
  }
  return undefined
}
