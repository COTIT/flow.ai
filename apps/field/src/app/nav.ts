import {
  canAccessSurface,
  resolveLanding,
  type ActiveMembership,
  type Surface,
} from '@/app/membership'
import { resourcesEnabled } from '@/lib/features'

/** Bottom tab bar for the mobile-first surfaces, sidebar for desktop-first ops. */
export type NavLayout = 'tabs' | 'sidebar' | 'none'

/** A count shown on a nav item. In-app only: the MVP sends no SMS or push. */
export type NavBadgeSource = 'surveys'

export interface NavItem {
  to: string
  /** i18n key, resolved by the component. Never a literal string. */
  labelKey: string
  badge?: NavBadgeSource
  /** Held back by a release flag (`@/lib/features`) — hidden when it is off. */
  feature?: 'resources'
}

const SURFACE_PREFIX: Array<[Surface, string]> = [
  ['farmer', '/farm'],
  ['officer', '/officer'],
  ['ops', '/ops'],
]

/**
 * Which surface a path belongs to.
 *
 * Matches on a path segment boundary, so `/farmers-market` is not the farmer
 * surface and `/operations` is not ops.
 */
export function surfaceForPath(pathname: string): Surface | undefined {
  for (const [surface, prefix] of SURFACE_PREFIX) {
    if (pathname === prefix || pathname.startsWith(`${prefix}/`)) return surface
  }
  return undefined
}

/**
 * Spec 4.1: "Farmer and Officer get a bottom tab bar; Ops gets a sidebar."
 *
 * Keyed off the current surface rather than the role set, because a user may
 * hold several roles and the URL is where the active one lives — selection is
 * held in memory and the URL, never in a token.
 */
export function navLayoutForPath(pathname: string): NavLayout {
  return navLayoutForSurface(surfaceForPath(pathname))
}

/** The layout a given surface uses. Spec 4.1, expressed once. */
export function navLayoutForSurface(surface: Surface | undefined): NavLayout {
  if (!surface) return 'none'
  return surface === 'ops' ? 'sidebar' : 'tabs'
}

/**
 * Which surface's nav to SHOW, which is not always the surface of the path.
 *
 * QA-FINDINGS.md #6: deciding from the URL alone stranded ops and admin users.
 * The Tower's traceability claim (spec §8.2) sends them from a headline into
 * `/officer/people/$personId` — an officer path they are legitimately allowed
 * to read — and they arrived to find the ops sidebar replaced by the officer's
 * tab bar, two of whose three tabs were placeholders. No route back to
 * Requests, Demand or the Tower except the browser's back button.
 *
 * So a session that is visiting another surface it can also read keeps its
 * OWN nav. Spec 4.1's tabs-vs-sidebar split is preserved — it is keyed to the
 * surface the user works in, rather than to whichever record they opened.
 */
export function navSurfaceFor(
  pathname: string,
  memberships: ActiveMembership[],
): Surface | undefined {
  const pathSurface = surfaceForPath(pathname)
  if (!pathSurface) return undefined

  const homeSurface = surfaceForPath(resolveLanding(memberships).to)
  if (
    homeSurface &&
    homeSurface !== pathSurface &&
    // Only for a surface they may actually read. Someone who hand-typed a path
    // they cannot open is about to be redirected, and until then gets no nav
    // rather than a nav for a surface they do not hold.
    canAccessSurface(memberships, pathSurface)
  ) {
    return homeSurface
  }

  return pathSurface
}

// Only routes that exist. Screens still to be built are listed as they land,
// so nav never points at a route the router cannot resolve.
const SURFACE_ITEMS: Record<Surface, NavItem[]> = {
  farmer: [
    { to: '/farm/my-farm', labelKey: 'nav.myFarm' },
    { to: '/farm/equipment', labelKey: 'nav.equipment', feature: 'resources' },
    { to: '/farm/requests', labelKey: 'nav.requests', feature: 'resources' },
    { to: '/farm/opportunities', labelKey: 'nav.opportunities' },
    { to: '/farm/surveys', labelKey: 'nav.surveys', badge: 'surveys' },
  ],
  officer: [
    { to: '/officer/register', labelKey: 'nav.register' },
    { to: '/officer/people', labelKey: 'nav.people' },
    { to: '/officer/verify', labelKey: 'nav.verify' },
    { to: '/officer/redeem', labelKey: 'nav.redeem' },
  ],
  ops: [
    { to: '/ops/requests', labelKey: 'nav.requests', feature: 'resources' },
    { to: '/ops/demand', labelKey: 'nav.demand' },
    { to: '/ops/catalogue', labelKey: 'nav.catalogue', feature: 'resources' },
    { to: '/ops/buyers', labelKey: 'nav.buyers' },
    { to: '/ops/villages', labelKey: 'nav.villages' },
    { to: '/ops/surveys', labelKey: 'nav.surveys' },
    { to: '/ops/supply-links', labelKey: 'nav.supplyLinks' },
    { to: '/ops/dispatch', labelKey: 'nav.dispatch' },
    { to: '/ops/tower', labelKey: 'nav.tower' },
  ],
}

/**
 * Nav for a surface, or nothing when the session does not open it — so nav
 * never advertises a route the guard would immediately bounce.
 */
export function navItemsFor(
  surface: Surface,
  memberships: ActiveMembership[],
  resources = resourcesEnabled,
): NavItem[] {
  if (!canAccessSurface(memberships, surface)) return []
  return SURFACE_ITEMS[surface].filter((item) => !item.feature || resources)
}
