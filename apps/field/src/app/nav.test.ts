import { describe, expect, test } from 'vitest'

import {
  navItemsFor,
  navLayoutForPath,
  navLayoutForSurface,
  navSurfaceFor,
  surfaceForPath,
} from '@/app/nav'
import type { ActiveMembership } from '@/app/membership'

const m = (role: ActiveMembership['role']): ActiveMembership => ({
  id: crypto.randomUUID(),
  role,
  project_id: '20000000-0000-4000-8000-000000000001',
  village_id: null,
  revoked_at: null,
})

describe('surfaceForPath', () => {
  test('maps each route group to its surface', () => {
    expect(surfaceForPath('/farm')).toBe('farmer')
    expect(surfaceForPath('/farm/my-farm')).toBe('farmer')
    expect(surfaceForPath('/officer')).toBe('officer')
    expect(surfaceForPath('/officer/register')).toBe('officer')
    expect(surfaceForPath('/ops')).toBe('ops')
    expect(surfaceForPath('/ops/tower')).toBe('ops')
    expect(surfaceForPath('/ops/tower/energy')).toBe('ops')
  })

  test('auth routes belong to no surface', () => {
    expect(surfaceForPath('/login')).toBeUndefined()
    expect(surfaceForPath('/select-role')).toBeUndefined()
    expect(surfaceForPath('/no-access')).toBeUndefined()
    expect(surfaceForPath('/')).toBeUndefined()
  })

  test('a path that merely starts with the same letters is not a match', () => {
    expect(surfaceForPath('/farmers-market')).toBeUndefined()
    expect(surfaceForPath('/operations')).toBeUndefined()
  })
})

describe('navLayoutForPath', () => {
  // Spec 4.1: Farmer and Officer get a bottom tab bar; Ops gets a sidebar.
  // The farmer and officer surfaces are mobile-first, ops is desktop-first.
  test('farmer and officer surfaces use a bottom tab bar', () => {
    expect(navLayoutForPath('/farm')).toBe('tabs')
    expect(navLayoutForPath('/farm/equipment')).toBe('tabs')
    expect(navLayoutForPath('/officer')).toBe('tabs')
  })

  test('the ops surface uses a sidebar, Tower included', () => {
    expect(navLayoutForPath('/ops')).toBe('sidebar')
    expect(navLayoutForPath('/ops/tower')).toBe('sidebar')
  })

  test('signed-out and auth screens get no nav at all', () => {
    expect(navLayoutForPath('/login')).toBe('none')
    expect(navLayoutForPath('/no-access')).toBe('none')
  })
})

describe('navItemsFor', () => {
  test('the farmer tab bar is the farmer surface only', () => {
    const items = navItemsFor('farmer', [m('farmer')])
    expect(items.map((i) => i.to)).toEqual([
      '/farm/my-farm',
      '/farm/equipment',
      '/farm/requests',
      '/farm/opportunities',
      '/farm/surveys',
    ])
  })

  // In-app only: the MVP sends no SMS and no push. The badge is the whole of
  // "farmers are notified".
  test('the farmer Surveys tab carries the new-surveys badge, and nothing else does', () => {
    const items = navItemsFor('farmer', [m('farmer')])
    expect(items.filter((i) => i.badge).map((i) => [i.to, i.badge])).toEqual([
      ['/farm/surveys', 'surveys'],
    ])
  })

  test('the officer tab bar is the officer surface only', () => {
    const items = navItemsFor('officer', [m('field_officer')])
    expect(items.map((i) => i.to)).toEqual([
      '/officer/register',
      '/officer/people',
      '/officer/verify',
      '/officer/redeem',
    ])
  })

  test('the ops sidebar covers the ops surface and the Tower', () => {
    const items = navItemsFor('ops', [m('ops')])
    expect(items.map((i) => i.to)).toEqual([
      '/ops/requests',
      '/ops/demand',
      '/ops/catalogue',
      '/ops/buyers',
      '/ops/villages',
      '/ops/surveys',
      '/ops/tower',
    ])
  })

  test('admin gets the same sidebar as ops', () => {
    expect(navItemsFor('ops', [m('admin')]).map((i) => i.to)).toEqual(
      navItemsFor('ops', [m('ops')]).map((i) => i.to),
    )
  })

  // A surface the session does not open lists nothing, so nav never advertises
  // a route the guard would immediately bounce.
  test('a surface the session does not open lists nothing', () => {
    expect(navItemsFor('ops', [m('farmer')])).toEqual([])
    expect(navItemsFor('farmer', [m('ops')])).toEqual([])
  })
})

/**
 * QA-FINDINGS.md #6. `navLayoutForPath` keys off the URL alone, so an ops user
 * who drilled from a Tower headline into `/officer/people/<id>` — a path the
 * Tower's own traceability claim (spec §8.2) sends them down — lost the ops
 * sidebar and was handed the officer's tab bar instead. Two of those three
 * tabs were placeholders, so the only route back was the browser's back button.
 */
describe('navSurfaceFor', () => {
  test('a session on its own surface keeps that surface', () => {
    expect(navSurfaceFor('/officer/register', [m('field_officer')])).toBe('officer')
    expect(navSurfaceFor('/farm/my-farm', [m('farmer')])).toBe('farmer')
    expect(navSurfaceFor('/ops/requests', [m('ops')])).toBe('ops')
  })

  test('ops viewing an officer screen keeps the ops nav', () => {
    expect(navSurfaceFor('/officer/people/abc', [m('ops')])).toBe('ops')
    expect(navSurfaceFor('/officer/cycles/abc', [m('admin')])).toBe('ops')
  })

  // Not symmetrical with the officer case, and deliberately so. The officer
  // surface is readable by ops and admin — the Tower drills into it — but the
  // FARMER surface is farmer-only, and the guard redirects ops away (asserted
  // in e2e/my-farm.spec.ts). So there is no ops-on-a-farmer-screen state to
  // keep a nav for; what matters is that they are not handed the farmer's tabs
  // on the way out.
  test('ops on a farmer path is given no farmer nav', () => {
    const surface = navSurfaceFor('/farm/my-farm', [m('ops')])
    expect(navItemsFor(surface!, [m('ops')])).toEqual([])
  })

  // The guard bounces them, and until it does they get no nav rather than
  // someone else's — nav must never advertise a surface the session cannot open.
  test('a farmer hand-typing an ops path is not given the ops nav', () => {
    expect(navItemsFor(navSurfaceFor('/ops/requests', [m('farmer')])!, [m('farmer')])).toEqual([])
  })

  test('auth routes still belong to no surface', () => {
    expect(navSurfaceFor('/login', [m('ops')])).toBeUndefined()
  })
})

describe('navLayoutForSurface', () => {
  test('ops gets a sidebar, the mobile-first surfaces get tabs', () => {
    expect(navLayoutForSurface('ops')).toBe('sidebar')
    expect(navLayoutForSurface('officer')).toBe('tabs')
    expect(navLayoutForSurface('farmer')).toBe('tabs')
    expect(navLayoutForSurface(undefined)).toBe('none')
  })
})

describe('the resource release flag', () => {
  // 25 Oct 2026: equipment, loans, requests and the catalogue are held back
  // from the first production release; surveys and opportunities lead.
  test('with it off, the farmer tabs are My farm, Opportunities and Surveys', () => {
    expect(navItemsFor('farmer', [m('farmer')], false).map((i) => i.to)).toEqual([
      '/farm/my-farm',
      '/farm/opportunities',
      '/farm/surveys',
    ])
  })

  test('with it off, ops has no Requests or Catalogue', () => {
    const paths = navItemsFor('ops', [m('ops')], false).map((i) => i.to)
    expect(paths).not.toContain('/ops/requests')
    expect(paths).not.toContain('/ops/catalogue')
    expect(paths).toContain('/ops/surveys')
  })

  test('with it on, nothing is hidden', () => {
    expect(navItemsFor('farmer', [m('farmer')], true).map((i) => i.to)).toContain('/farm/equipment')
    expect(navItemsFor('ops', [m('ops')], true).map((i) => i.to)).toContain('/ops/catalogue')
  })
})
