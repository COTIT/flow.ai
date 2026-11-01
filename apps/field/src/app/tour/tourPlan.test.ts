import { describe, expect, test } from 'vitest'

import {
  ALL_CHAPTERS,
  chaptersFor,
  indexAfterSkip,
  isArrivalOnly,
  isPatternRoute,
  planFor,
  routeMatches,
} from '@/app/tour/tourPlan'
import type { Chapter } from '@/app/tour/tourSteps'

/**
 * A tour is a list of stops in chapters, and which stops a person is shown
 * depends on who they are and where the app is running. That decision is pure,
 * so it is settled here rather than through a rendered tour.
 */
const step = (testId: string, route = '/x', extra: Record<string, unknown> = {}) => ({
  testId,
  route,
  titleKey: `t.${testId}`,
  bodyKey: `b.${testId}`,
  ...extra,
})

const CHAPTERS: Chapter[] = [
  { id: 'welcome', titleKey: 'c.welcome', steps: [step('a'), step('b')] },
  {
    id: 'people',
    titleKey: 'c.people',
    steps: [
      step('list', '/people', { open: '[data-testid="row"]' }),
      step('detail', '/people/$personId'),
      step('more', '/people/$personId'),
    ],
  },
  {
    id: 'authoring',
    titleKey: 'c.authoring',
    audience: 'admin',
    steps: [step('editor', '/surveys')],
  },
  {
    id: 'next',
    titleKey: 'c.next',
    steps: [step('handoff', '/', { demoOnly: true }), step('plain', '/')],
  },
]

const ops = { isAdmin: false, isDemo: false, resources: true }
const admin = { isAdmin: true, isDemo: false, resources: true }
const demo = { isAdmin: false, isDemo: true, resources: true }
const release = { isAdmin: false, isDemo: false, resources: false }

describe('a route with a parameter is a pattern, not a place to travel to', () => {
  test('knows which routes are patterns', () => {
    expect(isPatternRoute('/ops/requests/$requestId')).toBe(true)
    expect(isPatternRoute('/ops/requests')).toBe(false)
  })

  test('a pattern matches any one segment in the parameter’s place', () => {
    expect(routeMatches('/officer/people/$personId', '/officer/people/8f2c-01')).toBe(true)
    expect(routeMatches('/officer/people/$personId', '/officer/people')).toBe(false)
    expect(routeMatches('/officer/people/$personId', '/officer/people/8f2c/edit')).toBe(false)
  })

  test('a plain route matches only itself', () => {
    expect(routeMatches('/ops/requests', '/ops/requests')).toBe(true)
    expect(routeMatches('/ops/requests', '/ops/requests/12')).toBe(false)
  })
})

describe('a stop the tour is taken to rather than travels to', () => {
  test('a route with a parameter is one', () => {
    expect(isArrivalOnly({ route: '/ops/requests/$requestId' })).toBe(true)
  })

  // The Tower's drill-downs are plain routes, but only make sense with the
  // village that was picked — a bare URL would lose it.
  test('and so is a plain route flagged as reached by a click', () => {
    expect(isArrivalOnly({ route: '/ops/tower/production', arrive: true })).toBe(true)
    expect(isArrivalOnly({ route: '/ops/tower/production' })).toBe(false)
  })
})

describe('which chapters a person is offered', () => {
  test('an admin-only chapter is hidden from everyone else', () => {
    expect(chaptersFor(CHAPTERS, ops).map((c) => c.id)).not.toContain('authoring')
    expect(chaptersFor(CHAPTERS, admin).map((c) => c.id)).toContain('authoring')
  })

  test('a demo-only stop is dropped outside the demo, and the chapter goes with it if empty', () => {
    const outside = chaptersFor(CHAPTERS, ops).find((c) => c.id === 'next')
    expect(outside?.steps.map((s) => s.testId)).toEqual(['plain'])

    const only: Chapter[] = [{ id: 'x', titleKey: 'c.x', steps: [step('h', '/', { demoOnly: true })] }]
    expect(chaptersFor(only, ops)).toEqual([])
    expect(chaptersFor(only, demo)).toHaveLength(1)
  })
})

// 25 Oct 2026: equipment, requests and the catalogue sit behind a release
// flag, and the tour must not travel to a screen the router would bounce.
describe('the resource release flag', () => {
  const flagged: Chapter[] = [
    { id: 'home', titleKey: 'c.home', steps: [step('summary'), step('latest', '/', { feature: 'resources' })] },
    { id: 'equipment', titleKey: 'c.eq', feature: 'resources', steps: [step('list', '/farm/equipment')] },
  ]

  test('with it off, a flagged chapter and a flagged stop are dropped', () => {
    const offered = chaptersFor(flagged, release)
    expect(offered.map((c) => c.id)).toEqual(['home'])
    expect(offered[0].steps.map((s) => s.testId)).toEqual(['summary'])
  })

  test('with it on, both are played', () => {
    expect(planFor(flagged, ALL_CHAPTERS, ops).map((s) => s.testId)).toEqual(['summary', 'latest', 'list'])
  })
})

describe('the stops a chapter plays', () => {
  test('one chapter plays only its own stops, each knowing which chapter it belongs to', () => {
    const plan = planFor(CHAPTERS, 'people', ops)
    expect(plan.map((s) => s.testId)).toEqual(['list', 'detail', 'more'])
    expect(new Set(plan.map((s) => s.chapterKey))).toEqual(new Set(['c.people']))
  })

  test('"all" plays every chapter offered, in order', () => {
    const plan = planFor(CHAPTERS, ALL_CHAPTERS, ops)
    expect(plan.map((s) => s.testId)).toEqual(['a', 'b', 'list', 'detail', 'more', 'plain'])
    expect(planFor(CHAPTERS, ALL_CHAPTERS, admin).map((s) => s.testId)).toContain('editor')
    expect(planFor(CHAPTERS, ALL_CHAPTERS, demo).map((s) => s.testId)).toContain('handoff')
  })

  test('a chapter that does not exist, or is not yours, plays nothing', () => {
    expect(planFor(CHAPTERS, 'nope', ops)).toEqual([])
    expect(planFor(CHAPTERS, 'authoring', ops)).toEqual([])
  })
})

/**
 * When a stop opens a row and there is no row, the screens behind it cannot be
 * shown. Skipping only the next stop would land on a detail screen the tour
 * never opened, and wait for it to time out.
 */
describe('moving past a stop that cannot be shown', () => {
  const plan = planFor(CHAPTERS, ALL_CHAPTERS, ops)
  const at = (testId: string) => plan.findIndex((s) => s.testId === testId)

  test('an ordinary stop is skipped on its own', () => {
    expect(indexAfterSkip(plan, at('a'))).toBe(at('b'))
  })

  test('a stop that opens something takes the screens it opens with it', () => {
    expect(indexAfterSkip(plan, at('list'))).toBe(at('plain'))
  })

  test('a plain route flagged as reached by a click goes with it', () => {
    const flagged = planFor(
      [
        {
          id: 't',
          titleKey: 'c.t',
          steps: [
            step('tower', '/tower', { open: '[data-testid="drill"]' }),
            step('drill', '/tower/production', { arrive: true }),
            step('after', '/other'),
          ],
        },
      ],
      ALL_CHAPTERS,
      ops,
    )
    expect(indexAfterSkip(flagged, 0)).toBe(2)
  })

  test('skipping the last stop runs off the end, which ends the tour', () => {
    expect(indexAfterSkip(plan, plan.length - 1)).toBe(plan.length)
  })
})
