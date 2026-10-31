import { expect, test, type Page } from '@playwright/test'

import { ALL_CHAPTERS, chaptersFor, isArrivalOnly, planFor } from '../src/app/tour/tourPlan'
import { CHAPTERS } from '../src/app/tour/tourSteps'
import { tr, type Lng } from './support/i18n'

/**
 * The guided tour, from the only angle that proves anything: a real first
 * visit, in a real browser, with the spotlight actually cut — and then every
 * chapter of every role played to its end against the seeded app.
 *
 * Every other spec starts with the tours already taken (`playwright.config.ts`).
 * This one opts out — a genuinely empty storage state is what a new user has.
 *
 * The claims below are shaped by what this file FAILED to claim the first time.
 * It asserted that the bubble was gone at the end, which is true of a tour that
 * finished and equally true of one that died with its overlay still up: a navy
 * sheet over the whole page, `pointer-events: auto`, swallowing every click.
 * That shipped, and came back as "the screen stays blue and I cannot click
 * anything, on desktop and mobile". So the end of a tour is asserted by what
 * the user can DO afterwards, not by what is no longer drawn.
 *
 * What the unit tests cannot prove is that an anchor sits on the screen the
 * stop names; playing every chapter does. It is also what proves the tour only
 * READS: the whole suite runs with a guard on the network, and any write the
 * tour causes fails the test that caused it.
 */
test.use({ storageState: { cookies: [], origins: [] } })

const PASSWORD = 'demo1234'

// The tour's own strings are read from the language bundle through tr(), in
// whichever language the app is showing (see langOf), so this file follows the
// Swahili draft as it lands.
const WHO = {
  officer: {
    email: 'officer.ilundo@demo.ruaha360.test',
    surface: 'officer' as const,
    isAdmin: false,
    landing: /\/officer$/,
    /** A link that is not where the tour left you, to prove the page still answers. */
    thenClick: 'nav.people',
    thenAt: /\/officer\/people/,
  },
  ops: {
    email: 'ops@demo.ruaha360.test',
    surface: 'ops' as const,
    isAdmin: false,
    landing: /\/ops$/,
    thenClick: 'nav.buyers',
    thenAt: /\/ops\/buyers/,
  },
  admin: {
    email: 'admin@demo.ruaha360.test',
    surface: 'ops' as const,
    isAdmin: true,
    landing: /\/ops$/,
    thenClick: 'nav.buyers',
    thenAt: /\/ops\/buyers/,
  },
  farmer: {
    email: 'neema@demo.ruaha360.test',
    surface: 'farmer' as const,
    isAdmin: false,
    landing: /\/farm$/,
    thenClick: 'nav.equipment',
    thenAt: /\/farm\/equipment/,
  },
} as const

type Role = keyof typeof WHO
const ROLES = Object.keys(WHO) as Role[]

async function signIn(page: Page, who: Role) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill(WHO[who].email)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(WHO[who].landing)
}

/**
 * The language the app is actually showing. Accounts are seeded 'sw' or 'en',
 * but app_user.locale is a shared, writable row — another spec (or a person)
 * can leave it changed — so the text expected here follows what was applied.
 */
async function langOf(page: Page): Promise<Lng> {
  const applied = await page.locator('html').getAttribute('lang')
  return applied === 'sw' ? 'sw' : 'en'
}

/**
 * The overlay is the part that swallows clicks, so its absence is asserted
 * directly — by class, because it is the library's element and carries no
 * test id of ours.
 */
const overlay = (page: Page) => page.locator('.react-joyride__overlay')

/** Whether this build shows the demo banner — `demoOnly` stops depend on it. */
async function isDemo(page: Page) {
  return (await page.getByTestId('demo-banner').count()) > 0
}

/**
 * Whether this build shows the resource chain — `feature: 'resources'` stops
 * depend on it. Read from the nav, as the demo banner is: the e2e process does
 * not see the build's environment.
 */
async function hasResources(page: Page) {
  return (await page.locator('a[href="/farm/equipment"], a[href="/ops/catalogue"]').count()) > 0
}

/**
 * What the tour is allowed to cause on the server: nothing. A write to a table,
 * or a call to a function that changes something, is a one-way action the tour
 * performed on a seeded record — the thing it must never do.
 */
const WRITING_RPC =
  /verify|decide|decision|review|redeem|publish|void|close|register|issue|reset|submit|save|create|attach|withdraw|release|answer|password|update|delete|set_/i

function watchForWrites(page: Page): string[] {
  const offending: string[] = []
  page.on('request', (request) => {
    const { pathname } = new URL(request.url())
    if (!pathname.includes('/rest/v1/')) return
    if (['GET', 'HEAD', 'OPTIONS'].includes(request.method())) return
    const rpc = pathname.split('/rpc/')[1]
    if (rpc && !WRITING_RPC.test(rpc)) return
    offending.push(`${request.method()} ${pathname}`)
  })
  return offending
}

/** The header button, or — on a phone — the one folded into the user menu. */
async function openTourMenu(page: Page) {
  const visibleButton = page.locator('[data-testid="tour-restart"]:visible')
  const trigger = page.getByTestId('user-menu-trigger')
  // The header fills in once the session has loaded; after a reload it has not.
  await expect
    .poll(async () => (await visibleButton.count()) + ((await trigger.isVisible()) ? 1 : 0))
    .toBeGreaterThan(0)
  if ((await visibleButton.count()) === 0) await trigger.click()
  await visibleButton.first().click()
  await expect(page.getByTestId('tour-menu')).toBeVisible()
}

async function playChapter(page: Page, chapterId: string) {
  await openTourMenu(page)
  await page.getByTestId(`tour-chapter-${chapterId}`).click()
}

/** Signing in opens the welcome chapter by itself; put it away to start clean. */
async function skipWelcome(page: Page) {
  await expect(page.getByTestId('tour-tooltip')).toBeVisible()
  await page.getByTestId('tour-skip').click()
  await expect(overlay(page)).toHaveCount(0)
}

/** "Step n of m", in whichever language the account is seeded with. */
async function position(page: Page) {
  const text = (await page.getByTestId('tour-progress').textContent()) ?? ''
  const numbers = text.match(/\d+/g)?.map(Number) ?? []
  if (numbers.length < 2) throw new Error(`cannot read a position from "${text}"`)
  return { n: numbers[0], m: numbers[1], text }
}

/**
 * Wait for the bubble to show a stop other than `previous`, or for the tour to
 * have ended. A stop on another screen has to wait for a route change and the
 * read behind it, and the bubble is not drawn while the tour waits for its
 * target — so the patience here is deliberately longer than the tour's own.
 * "Ended" is only believed after a long quiet: between two stops the bubble is
 * briefly gone while the next one is prepared.
 */
async function nextBubble(page: Page, previous: string | null): Promise<'moved' | 'ended'> {
  const tooltip = page.getByTestId('tour-tooltip')
  const deadline = Date.now() + 45_000
  let quietSince: number | null = null

  while (Date.now() < deadline) {
    if (await tooltip.isVisible()) {
      quietSince = null
      const { text } = await position(page)
      if (previous === null || text !== previous) return 'moved'
    } else {
      quietSince ??= Date.now()
      if (previous !== null && Date.now() - quietSince > 14_000) return 'ended'
    }
    await page.waitForTimeout(150)
  }
  throw new Error(`the tour neither moved on from "${previous}" nor ended`)
}

/**
 * What the presenter does to satisfy a stop that waits for them. Keyed by the
 * gate's test id, because that is what the stop names.
 */
const GATE_ACTIONS: Record<string, (page: Page) => Promise<void>> = {
  // The Tower shows nothing until a village is chosen.
  'tile-production': async (page) => {
    const picker = page.getByTestId('tower-village')
    const tag = await picker.evaluate((el) => el.tagName)
    if (tag === 'SELECT') {
      await picker.selectOption({ label: 'Ilundo' })
    } else {
      await picker.click()
      await page.getByRole('option', { name: /Ilundo/ }).first().click()
    }
  },
  // The estimate appears once the fields describe something possible.
  'estimate-panel': async (page) => {
    await page.getByTestId('request-quantity').fill('1')
    await page.getByTestId('request-hours').fill('4')
    await page.getByTestId('request-days').fill('5')
  },
  // The audit trail appears once a voucher is picked.
  'voucher-panel': async (page) => {
    await page.getByTestId('voucher-row').first().click()
  },
}

interface Walked {
  seen: number[]
  ended: 'done' | 'skipped-to-end'
}

/**
 * Play the chapter that was just chosen, one click at a time, asserting the
 * number and the anchor at every stop.
 *
 * Asserting each stop in turn is the point rather than ceremony: the farmer
 * tour once raced from stop 1 to stop 5 on its own, because a route crossing
 * briefly has no target and "target not found" was wired to advance. A test
 * that only checked the end would have called that a pass.
 */
async function walk(page: Page, role: Role, chapterId: string): Promise<Walked> {
  const { surface, isAdmin } = WHO[role]
  const lng = await langOf(page)
  const plan = planFor(CHAPTERS[surface], chapterId, { isAdmin, isDemo: await isDemo(page), resources: await hasResources(page) })
  expect(plan.length, `${role}/${chapterId} has no stops here`).toBeGreaterThan(0)

  const seen: number[] = []
  let previous: string | null = null
  let ended: Walked['ended'] = 'skipped-to-end'

  for (let guard = 0; guard < plan.length + 4; guard += 1) {
    if ((await nextBubble(page, previous)) === 'ended') break

    const { n, m, text } = await position(page)
    expect(m, 'the counter counts this chapter’s stops').toBe(plan.length)
    const step = plan[n - 1]
    seen.push(n - 1)

    const tooltip = page.getByTestId('tour-tooltip')
    const anchor = page.getByTestId(step.testId).first()
    await expect(tooltip, `${step.testId}: bubble`).toBeInViewport()
    await expect(anchor, `${step.testId}: anchor`).toBeVisible()
    await expect(anchor, `${step.testId}: anchor`).toBeInViewport()
    await expect(page.getByTestId('tour-tryit')).toHaveCount(step.tryIt ? 1 : 0)

    const next = page.getByTestId('tour-next')
    if (step.gate) {
      // Next waits for the person, and says why — unless the page already
      // shows what the stop asks for (a form that starts with a valid estimate).
      const alreadyThere = (await page.getByTestId(step.gate).count()) > 0
      if (!alreadyThere) {
        await expect(next).toBeDisabled()
        await expect(page.getByTestId('tour-gate-hint')).toBeVisible()
        const action = GATE_ACTIONS[step.gate]
        if (!action) throw new Error(`no e2e action is defined for gate "${step.gate}"`)
        await action(page)
      }
      await expect(next).toBeEnabled()
    }

    previous = text
    if (n === m) {
      // The last stop finishes rather than promising more.
      await expect(next).toHaveText(tr(lng, 'tour.close'))
      await expect(page.getByTestId('tour-skip')).toHaveCount(0)
      await next.click()
      ended = 'done'
      break
    }
    await next.click()
  }

  // The bubble going away is not the claim. These two are.
  await expect(overlay(page)).toHaveCount(0)
  await expect(page.getByTestId('tour-tooltip')).toHaveCount(0)

  if (process.env.TOUR_DEBUG) console.log(`${role}/${chapterId} saw:`, seen.map((i) => plan[i].testId).join(', '))
  // Every stop that is not allowed to be missing was shown.
  const missed = plan
    .map((step, index) => ({ step, index }))
    .filter(({ step, index }) => !step.optional && !seen.includes(index))
    .map(({ step }) => step.testId)
  expect(missed, `${role}/${chapterId}: stops that should have been shown were skipped`).toEqual([])

  return { seen, ended }
}

/** The page answers an ordinary click, which is what "I cannot click anything" was about. */
async function assertPageIsUsable(page: Page, role: Role) {
  const { thenClick, thenAt } = WHO[role]
  const lng = await langOf(page)
  // Inside the nav: a farmer's home screen has links of its own with similar names.
  await page
    .locator('[data-testid="nav-tabs"], [data-testid="nav-sidebar"]')
    .getByRole('link', { name: tr(lng, thenClick) })
    .click()
  await expect(page).toHaveURL(thenAt)
}

const OFFERED = (role: Role) =>
  chaptersFor(CHAPTERS[WHO[role].surface], { isAdmin: WHO[role].isAdmin, isDemo: true, resources: true })

test.describe('the first visit', () => {
  // Every role gets one, and it opens by itself — a tour nobody knows is there
  // is not a tour. What opens is the short welcome, not everything.
  for (const role of ROLES) {
    test(`opens the welcome chapter for a ${role} who has never seen the tour`, async ({ page }) => {
      await signIn(page, role)
      const welcome = planFor(CHAPTERS[WHO[role].surface], 'welcome', {
        isAdmin: WHO[role].isAdmin,
        isDemo: await isDemo(page), resources: await hasResources(page),
      })

      await expect(page.getByTestId('tour-tooltip')).toBeVisible()
      await expect(page.getByTestId('tour-progress')).toHaveText(
        tr(await langOf(page), 'tour.progress', { step: 1, total: welcome.length }),
      )
      await expect(page.getByTestId('tour-tooltip')).toContainText(
        tr(await langOf(page), `tour.${WHO[role].surface}.welcomeTitle`),
      )
      // Nothing behind the first stop, so no Back to offer.
      await expect(page.getByTestId('tour-back')).toHaveCount(0)
    })
  }

  test('and does not open it a second time', async ({ page }) => {
    await signIn(page, 'officer')
    await skipWelcome(page)

    await page.reload()
    await expect(page.getByTestId('officer-home')).toBeVisible()
    await expect(page.getByTestId('tour-tooltip')).toHaveCount(0)
    await expect(overlay(page)).toHaveCount(0)
  })

  for (const role of ROLES) {
    test(`the ${role} welcome ends in a page that still answers`, async ({ page }) => {
      const writes = watchForWrites(page)
      await signIn(page, role)
      await expect(page.getByTestId('tour-tooltip')).toBeVisible()

      await walk(page, role, 'welcome')
      await assertPageIsUsable(page, role)

      // Having been taken, it is remembered across a reload.
      await page.goto('/')
      await expect(page).toHaveURL(WHO[role].landing)
      await expect(page.getByTestId('tour-tooltip')).toHaveCount(0)
      await expect(overlay(page)).toHaveCount(0)
      expect(writes, 'the tour only reads').toEqual([])
    })
  }
})

test.describe('the menu of chapters', () => {
  for (const role of ROLES) {
    test(`lists what a ${role} can play, and nothing they cannot`, async ({ page }) => {
      await signIn(page, role)
      await skipWelcome(page)
      await openTourMenu(page)

      const offered = OFFERED(role).map((chapter) => chapter.id)
      await expect(page.getByTestId('tour-chapter-all')).toBeVisible()
      for (const id of offered) await expect(page.getByTestId(`tour-chapter-${id}`)).toBeVisible()

      // The ops surface is shared: only an admin is offered the authoring chapter.
      if (role === 'ops') await expect(page.getByTestId('tour-chapter-authoring')).toHaveCount(0)
      if (role === 'admin') await expect(page.getByTestId('tour-chapter-authoring')).toBeVisible()
    })
  }

  test('can be closed without playing anything', async ({ page }) => {
    await signIn(page, 'ops')
    await skipWelcome(page)
    await openTourMenu(page)

    await page.getByTestId('tour-menu-close').click()
    await expect(page.getByTestId('tour-menu')).toHaveCount(0)
    await expect(page.getByTestId('tour-tooltip')).toHaveCount(0)
    await expect(overlay(page)).toHaveCount(0)
  })

  test('asking again from somewhere else plays from the first stop of that chapter', async ({ page }) => {
    await signIn(page, 'ops')
    await skipWelcome(page)

    // From elsewhere entirely: choosing a chapter has to travel to its first stop.
    await page.goto('/ops/villages')
    await playChapter(page, ALL_CHAPTERS)

    await expect(page).toHaveURL(/\/ops$/)
    await expect(page.getByTestId('tour-progress')).toContainText('1')
  })

  test('the button is there on every surface, and never on the login screen', async ({ page }) => {
    await page.goto('/login')
    await expect(page.getByTestId('tour-restart')).toHaveCount(0)

    await signIn(page, 'farmer')
    await expect(page.getByRole('banner').getByTestId('tour-restart')).toBeVisible()
  })
})

/**
 * Every chapter of every role, played to its end against the seeded app.
 *
 * All four are walked because the two that broke in the field — ops at stop
 * 4→5, farmer racing itself from stop 1 — were the two this file did not cover.
 */
for (const role of ROLES) {
  test.describe(`the ${role} chapters, one at a time`, () => {
    for (const chapter of OFFERED(role).filter((c) => c.id !== 'welcome')) {
      test(`${chapter.id}`, async ({ page }) => {
        const writes = watchForWrites(page)
        await signIn(page, role)
        await skipWelcome(page)

        const plan = planFor(CHAPTERS[WHO[role].surface], chapter.id, {
          isAdmin: WHO[role].isAdmin,
          isDemo: await isDemo(page), resources: await hasResources(page),
        })
        test.skip(plan.length === 0, 'this chapter is not shown in this build')

        await playChapter(page, chapter.id)
        await walk(page, role, chapter.id)
        await assertPageIsUsable(page, role)

        expect(writes, 'the tour only reads').toEqual([])
      })
    }
  })
}

/** The whole thing, end to end: chapter into chapter, ending on a usable page. */
for (const role of ROLES) {
  test(`the ${role} tour plays end to end`, async ({ page }) => {
    test.setTimeout(360_000)
    const writes = watchForWrites(page)
    await signIn(page, role)
    await skipWelcome(page)

    await playChapter(page, ALL_CHAPTERS)
    await walk(page, role, ALL_CHAPTERS)
    await assertPageIsUsable(page, role)

    expect(writes, 'the tour only reads').toEqual([])
  })
}

/**
 * The farmer and officer surfaces are phone apps. Their chapters are played at
 * phone width too: the tab bar covers the bottom of the screen, the bubble has
 * less room, and the tour button lives inside the user menu.
 */
test.describe('at phone width', () => {
  test.use({ viewport: { width: 390, height: 844 } })

  // What a first-time farmer or officer sees, on the device they actually use.
  for (const role of ['farmer', 'officer'] as const) {
    test(`${role}: the welcome opens by itself and ends on a page that answers`, async ({ page }) => {
      const writes = watchForWrites(page)
      await signIn(page, role)
      await expect(page.getByTestId('tour-tooltip')).toBeVisible()

      await walk(page, role, 'welcome')
      await assertPageIsUsable(page, role)
      expect(writes, 'the tour only reads').toEqual([])
    })
  }

  for (const role of ['farmer', 'officer'] as const) {
    for (const chapter of OFFERED(role).filter((c) => c.id !== 'welcome')) {
      test(`${role}: ${chapter.id}`, async ({ page }) => {
        const writes = watchForWrites(page)
        await signIn(page, role)
        await skipWelcome(page)

        const plan = planFor(CHAPTERS[WHO[role].surface], chapter.id, {
          isAdmin: false,
          isDemo: await isDemo(page), resources: await hasResources(page),
        })
        test.skip(plan.length === 0, 'this chapter is not shown in this build')

        await playChapter(page, chapter.id)
        await walk(page, role, chapter.id)
        await assertPageIsUsable(page, role)

        expect(writes, 'the tour only reads').toEqual([])
      })
    }
  }

  test('the officer submit stop is scrolled into view, not left under the tab bar', async ({ page }) => {
    await signIn(page, 'officer')
    await skipWelcome(page)
    await playChapter(page, 'register')

    const plan = planFor(CHAPTERS.officer, 'register', { isAdmin: false, isDemo: await isDemo(page), resources: await hasResources(page) })
    const submit = plan.findIndex((step) => step.testId === 'register-submit')
    expect(submit, 'the register chapter still explains the submit button').toBeGreaterThanOrEqual(0)

    for (let stop = 1; stop <= submit; stop += 1) {
      await expect(page.getByTestId('tour-progress')).toHaveText(
        tr(await langOf(page), 'tour.progress', { step: stop, total: plan.length }),
        { timeout: 20_000 },
      )
      await page.getByTestId('tour-next').click()
    }
    await expect(page.getByTestId('tour-progress')).toHaveText(
      tr(await langOf(page), 'tour.progress', { step: submit + 1, total: plan.length }),
      { timeout: 20_000 },
    )
    await expect(page.getByTestId('register-submit')).toBeInViewport()
    expect(await page.evaluate(() => window.scrollY)).toBeGreaterThan(0)
  })
})

test.describe('moving between screens', () => {
  test('Back travels to the screen the last stop was on', async ({ page }) => {
    await signIn(page, 'officer')
    await skipWelcome(page)
    await playChapter(page, ALL_CHAPTERS)

    const plan = planFor(CHAPTERS.officer, ALL_CHAPTERS, { isAdmin: false, isDemo: await isDemo(page), resources: await hasResources(page) })
    // The first stop that lives on a different static screen from the one before it.
    const crossing = plan.findIndex(
      (step, index) => index > 0 && !isArrivalOnly(step) && step.route !== plan[index - 1].route,
    )
    expect(crossing).toBeGreaterThan(0)

    for (let stop = 1; stop <= crossing; stop += 1) {
      await expect(page.getByTestId('tour-progress')).toHaveText(
        tr(await langOf(page), 'tour.progress', { step: stop, total: plan.length }),
        { timeout: 20_000 },
      )
      await page.getByTestId('tour-next').click()
    }
    await expect(page).toHaveURL(new RegExp(`${plan[crossing].route}(\\?.*)?$`), { timeout: 20_000 })
    await expect(page.getByTestId('tour-progress')).toHaveText(
      tr(await langOf(page), 'tour.progress', { step: crossing + 1, total: plan.length }),
      { timeout: 20_000 },
    )

    await page.getByTestId('tour-back').click()
    await expect(page).toHaveURL(new RegExp(`${plan[crossing - 1].route}(\\?.*)?$`))
    await expect(page.getByTestId('tour-progress')).toHaveText(
      tr(await langOf(page), 'tour.progress', { step: crossing, total: plan.length }),
      { timeout: 20_000 },
    )
  })

  // Back out of a screen the tour opened undoes the click, rather than
  // navigating afresh and losing what the click carried.
  test('Back from a screen the tour opened returns to the list it came from', async ({ page }) => {
    await signIn(page, 'officer')
    await skipWelcome(page)
    await playChapter(page, 'people')

    const plan = planFor(CHAPTERS.officer, 'people', { isAdmin: false, isDemo: await isDemo(page), resources: await hasResources(page) })
    const opened = plan.findIndex((step) => isArrivalOnly(step))
    expect(opened, 'the people chapter opens a person').toBeGreaterThan(0)

    for (let stop = 1; stop <= opened; stop += 1) {
      await expect(page.getByTestId('tour-progress')).toHaveText(
        tr(await langOf(page), 'tour.progress', { step: stop, total: plan.length }),
        { timeout: 20_000 },
      )
      // A stop that asks the person to try something does not wait for them.
      await page.getByTestId('tour-next').click()
    }
    await expect(page.getByTestId('tour-progress')).toHaveText(
      tr(await langOf(page), 'tour.progress', { step: opened + 1, total: plan.length }),
      { timeout: 20_000 },
    )
    await expect(page).toHaveURL(/\/officer\/people\/[^/]+$/)

    await page.getByTestId('tour-back').click()
    await expect(page).toHaveURL(/\/officer\/people$/)
    await expect(page.getByTestId('tour-progress')).toHaveText(
      tr(await langOf(page), 'tour.progress', { step: opened, total: plan.length }),
      { timeout: 20_000 },
    )
  })

  /**
   * A screen still loading its data must not lose the tour its place. The stop
   * is only shown once its own screen is the screen we are on, so a slow read
   * delays the bubble and never skips it.
   */
  test('a slow screen delays a stop rather than skipping it', async ({ page }) => {
    await page.route('**/rest/v1/v_demand_match*', async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 1500))
      await route.continue()
    })

    await signIn(page, 'ops')
    await skipWelcome(page)
    await playChapter(page, 'demand')

    await expect(page).toHaveURL(/\/ops\/demand/)
    await expect(page.getByTestId('tour-progress')).toContainText('1', { timeout: 20_000 })
    await expect(page.getByTestId('demand-table')).toBeVisible()
  })
})
