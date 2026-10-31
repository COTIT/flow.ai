import { describe, expect, test } from 'vitest'

import { isArrivalOnly } from '@/app/tour/tourPlan'
import { CHAPTERS, type Chapter, type TourStep } from '@/app/tour/tourSteps'
import { resolveLanding } from '@/app/membership'
import { code, read, sourceFiles } from '@/styles/design'
import en from '@/i18n/en/common.json'

/**
 * A guided tour is the one feature that breaks in total silence.
 *
 * It points at other people's markup by selector, and nothing in a refactor
 * tells you that `officer-unverified` was renamed — the tour simply finds no
 * element and either skips the step or parks an empty bubble in the middle of
 * the screen, which is worse than not having a tour. So the steps are DATA,
 * checked here against the app they describe: every target exists, every route
 * exists, every string is a key somebody can translate.
 *
 * Targets are `data-testid` on purpose. They are already a contract this repo
 * keeps — `pnpm e2e` fails when one moves — so the tour inherits that contract
 * instead of inventing a parallel one out of class names.
 *
 * What this cannot prove is that an anchor sits on the route the step names: a
 * test id that exists on another screen passes here. `e2e/tour.spec.ts` walks
 * every chapter of every role against the seeded app for that.
 */
const SURFACES = ['farmer', 'officer', 'ops'] as const

/**
 * Every test id the app renders. Two spellings, because shared components take
 * theirs as a prop — `<DataTable testId="requests-table">` puts the attribute
 * on markup that lives in another file entirely.
 */
const RENDERED_TEST_IDS = new Set(
  sourceFiles('src', ['.tsx'])
    .flatMap((file) => [...code(file).matchAll(/(?:data-testid|testId)="([a-z0-9-]+)"/g)])
    .map((match) => match[1]),
)

/** Every route the generated tree can resolve. */
const ROUTES = new Set(
  [...read('src/routeTree.gen.ts').matchAll(/'(\/[A-Za-z0-9\-/$]*)'/g)].map((match) => match[1]),
)

function lookup(key: string): unknown {
  return key.split('.').reduce<unknown>(
    (node, part) => (node && typeof node === 'object' ? (node as Record<string, unknown>)[part] : undefined),
    en,
  )
}

/** The test ids a CSS selector names. */
function testIdsIn(selector: string): string[] {
  return [...selector.matchAll(/data-testid="([a-z0-9-]+)"/g)].map((match) => match[1])
}

const everyChapter: Array<[string, Chapter]> = SURFACES.flatMap((surface) =>
  CHAPTERS[surface].map((chapter) => [`${surface}/${chapter.id}`, chapter] as [string, Chapter]),
)
const everyStep: Array<[string, TourStep]> = everyChapter.flatMap(([name, chapter]) =>
  chapter.steps.map((step) => [`${name}: ${step.testId}`, step] as [string, TourStep]),
)

describe('every role has a tour', () => {
  test.each(SURFACES)('%s has chapters', (surface) => {
    expect(CHAPTERS[surface].length, 'a tour of one chapter is not a menu').toBeGreaterThanOrEqual(3)
  })

  // What runs by itself on a first visit. It is short on purpose and it comes
  // first, so "play all" starts with it too.
  test.each(SURFACES)('the %s tour opens with a welcome chapter', (surface) => {
    expect(CHAPTERS[surface][0].id).toBe('welcome')
  })

  // Where the user actually lands after signing in. A tour whose first stop is
  // somewhere else opens by teleporting them, before it has explained anything.
  test.each([
    ['farmer', 'farmer'],
    ['officer', 'field_officer'],
    ['ops', 'ops'],
  ] as const)('the %s tour starts where that role lands', (surface, role) => {
    const landing = resolveLanding([{ role, revoked_at: null }] as never)
    expect(CHAPTERS[surface][0].steps[0].route).toBe(landing.to)
  })

  test.each(SURFACES)('%s chapters are named once', (surface) => {
    const ids = CHAPTERS[surface].map((chapter) => chapter.id)
    expect(ids).toEqual([...new Set(ids)])
  })

  test.each(everyChapter)('%s has stops and a name', (_name, chapter) => {
    expect(chapter.steps.length).toBeGreaterThan(0)
    expect(typeof lookup(chapter.titleKey), `${chapter.titleKey} is missing from en/common.json`).toBe('string')
  })
})

describe('every step points at something that exists', () => {
  test.each(everyStep)('%s', (_name, step) => {
    expect(RENDERED_TEST_IDS, `no element renders data-testid="${step.testId}"`).toContain(step.testId)
    expect(ROUTES, `${step.route} is not a route`).toContain(step.route)
  })

  test.each(everyStep.filter(([, step]) => step.open))('%s: what it opens is something the app renders', (_name, step) => {
    const named = testIdsIn(step.open!)
    expect(named.length, `open "${step.open}" names no test id`).toBeGreaterThan(0)
    for (const id of named) expect(RENDERED_TEST_IDS, `open names data-testid="${id}"`).toContain(id)
  })

  test.each(everyStep.filter(([, step]) => step.gate))('%s: what it waits for is something the app renders', (_name, step) => {
    expect(RENDERED_TEST_IDS, `gate names data-testid="${step.gate}"`).toContain(step.gate)
  })
})

describe('every step is written in words somebody can translate', () => {
  test.each(everyStep)('%s', (_name, step) => {
    expect(typeof lookup(step.titleKey), `${step.titleKey} is missing from en/common.json`).toBe('string')
    expect(typeof lookup(step.bodyKey), `${step.bodyKey} is missing from en/common.json`).toBe('string')
  })

  // The tour changes shape as chapters are added, and a sentence that counts
  // its own stops is wrong the next time anyone does.
  test.each(everyStep)('%s does not count its own stops', (_name, step) => {
    const words = `${lookup(step.titleKey)} ${lookup(step.bodyKey)}`
    expect(words).not.toMatch(/\b(short\s+)?(stops|steps)\b.*\bof\b|\b(one|two|three|four|five|six|seven|eight|nine|ten|\d+)\s+(short\s+)?(stops|steps)\b/i)
  })
})

/**
 * "As easy to follow as possible" is mostly this: a chapter that visits
 * Register, then People, then Register again has made the user travel twice to
 * say one thing. Each route gets one contiguous run of steps within a chapter.
 * Chapters may revisit a screen an earlier chapter used — that is what makes
 * each one playable on its own.
 */
describe('a chapter never doubles back', () => {
  test.each(everyChapter)('%s visits each screen once', (_name, chapter) => {
    const visits = chapter.steps.map((step) => step.route)
    const runs = visits.filter((route, index) => route !== visits[index - 1])
    expect(runs, 'a route is visited, left, and visited again').toEqual([...new Set(runs)])
  })

  test.each(everyChapter)('%s shows each anchor once', (_name, chapter) => {
    const ids = chapter.steps.map((step) => step.testId)
    expect(ids).toEqual([...new Set(ids)])
  })
})

/**
 * A route with a parameter is a request or a person the tour did not choose,
 * and a plain route flagged `arrive` only makes sense with what a click carried.
 * Neither can be travelled to, so the only way there is the stop before it
 * opening a row — and a chapter that starts on one, or reaches one any other
 * way, would play to a screen that never opens.
 */
describe('a screen the tour opens is only reached by opening it', () => {
  test.each(everyChapter)('%s', (_name, chapter) => {
    chapter.steps.forEach((step, index) => {
      const previous = chapter.steps[index - 1]
      if (isArrivalOnly(step)) {
        expect(index, `${step.testId} is on ${step.route}, which cannot be the first stop`).toBeGreaterThan(0)
        const reachable = previous.open || isArrivalOnly(previous)
        expect(reachable, `${step.testId} is on ${step.route}, and nothing before it opens that`).toBeTruthy()
      }
      if (step.open) {
        const following = chapter.steps[index + 1]
        expect(
          following && isArrivalOnly(following),
          `${step.testId} opens something, and no stop follows on the screen it opens`,
        ).toBe(true)
      }
    })
  })
})

/**
 * The tour explains one-way actions and never performs them. What it does let
 * a person try is typing, filtering and picking — things that keep nothing —
 * and only on the screen the stop is on.
 */
describe('what a stop is allowed to ask of the person', () => {
  test('a stop to try never opens another screen', () => {
    const offenders = everyStep.filter(([, step]) => step.tryIt && step.open).map(([name]) => name)
    expect(offenders, 'a stop to try stays where it is').toEqual([])
  })

  test('a stop that waits for the person is a stop to try', () => {
    const offenders = everyStep.filter(([, step]) => step.gate && !step.tryIt).map(([name]) => name)
    expect(offenders).toEqual([])
  })
})

/**
 * The product's own rules, held in the one place a tour would otherwise get
 * them wrong: a sentence that is quietly less careful than the screen it
 * describes. A stop anchored on any of these must say the thing the screen says.
 */
const MUST_SAY: Array<{ ids: RegExp; says: RegExp; rule: string }> = [
  { ids: /^(estimate-panel|review-estimate)$/, says: /estimate/i, rule: 'an estimate is called an estimate' },
  {
    ids: /^(equipment-price|equipment-list|catalogue-table|demand-table)$/,
    says: /indicative/i,
    rule: 'a price is indicative, and is not a quotation',
  },
  {
    ids: /^(tower-capacity|village-basis|villages-table|review-capacity-basis|tower-village)$/,
    says: /planned/i,
    rule: 'capacity is planned, never measured',
  },
  {
    ids: /^(farmer-opportunities|opportunity-detail|create-opportunity|opportunity-link)$/,
    says: /not a (sale|delivery|payment)|nothing is owed|not a sale/i,
    rule: 'an opportunity is not a sale, a delivery or a payment',
  },
]

describe('the tour says what the screen says', () => {
  for (const { ids, says, rule } of MUST_SAY) {
    const stops = everyStep.filter(([, step]) => ids.test(step.testId))
    test.each(stops.length ? stops : [['(no stop uses this yet)', null as never]])(`${rule}: %s`, (_name, step) => {
      if (!step) return
      const words = `${lookup(step.titleKey)} ${lookup(step.bodyKey)}`
      expect(words).toMatch(says)
    })
  }

  // The cash for answering a survey is a fixed amount handed over at the
  // office. Any of these words would turn it into something it is not.
  test.each(
    everyStep.filter(
      ([, step]) => /survey|voucher|redeem|audit|login/i.test(`${step.testId} ${step.bodyKey}`),
    ),
  )('%s never calls the incentive earnings, a wallet, a balance or a payment', (_name, step) => {
    const words = `${lookup(step.titleKey)} ${lookup(step.bodyKey)}`
    expect(words).not.toMatch(/\b(earn(s|ed|ing|ings)?|wallets?|balances?|payments?|paid out|salary|wages?)\b/i)
  })
})
