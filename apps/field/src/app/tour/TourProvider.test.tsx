import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

// The router's navigate returns a promise; the tour ends the tour if it
// rejects, so the stub has to be one.
const navigate = vi.fn(() => Promise.resolve())
let pathname = '/officer'
let joyride: Record<string, unknown> = {}

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigate,
  useLocation: () => ({ pathname }),
}))

vi.mock('react-joyride', () => ({
  // v3 exports no default — a named `Joyride`. The mock has to agree, or it
  // passes against a component the app cannot import.
  Joyride: (props: Record<string, unknown>) => {
    joyride = props
    // The real one renders our tooltip, and the tour watches for that bubble
    // to decide whether it is drawing anything at all.
    return (
      <div data-testid="joyride">{props.run ? <div data-testid="tour-tooltip" /> : null}</div>
    )
  },
  ACTIONS: { NEXT: 'next', PREV: 'prev' },
  EVENTS: {
    STEP_BEFORE: 'step:before',
    STEP_AFTER: 'step:after',
    TARGET_NOT_FOUND: 'error:target_not_found',
  },
  STATUS: { FINISHED: 'finished', SKIPPED: 'skipped' },
}))

/**
 * A small tour of its own. What is worth testing here is the DECISIONS — which
 * stops run, when to open a row, when to skip, when to remember — and none of
 * that should break because someone wrote a better chapter for the real app.
 */
vi.mock('@/app/tour/tourSteps', () => {
  const step = (route: string, testId: string, extra: Record<string, unknown> = {}) => ({
    route,
    testId,
    titleKey: 'tour.officer.welcomeTitle',
    bodyKey: 'tour.officer.welcomeBody',
    ...extra,
  })
  const officer = [
    {
      id: 'welcome',
      titleKey: 'tour.officer.chapter.welcome',
      steps: [step('/officer', 'officer-home', { placement: 'center' }), step('/officer', 'officer-unverified')],
    },
    {
      id: 'register',
      titleKey: 'tour.officer.chapter.register',
      steps: [step('/officer/register', 'register-progress'), step('/officer/register', 'register-submit')],
    },
    {
      id: 'people',
      titleKey: 'tour.officer.chapter.people',
      steps: [
        step('/officer/people', 'people-search', { open: '[data-testid="people-row"]' }),
        step('/officer/people/$personId', 'person-detail'),
        step('/officer/people/$personId', 'app-login', { optional: true }),
        step('/officer/people/$personId', 'person-outstanding'),
      ],
    },
    {
      id: 'tower',
      titleKey: 'tour.officer.chapter.redeem',
      steps: [
        step('/officer/verify', 'verify-queue', { tryIt: true, gate: 'verify-gate' }),
        step('/officer/verify', 'verify-after'),
      ],
    },
    {
      id: 'drill',
      titleKey: 'tour.officer.chapter.redeem',
      steps: [
        step('/officer/verify', 'verify-queue', { open: '[data-testid="drill-link"]' }),
        step('/officer/verify/drilled', 'verify-drilled', { arrive: true }),
      ],
    },
    {
      id: 'authoring',
      titleKey: 'tour.officer.chapter.verify',
      audience: 'admin',
      steps: [step('/officer/redeem', 'redeem-screen')],
    },
  ]
  const ops = [
    {
      id: 'welcome',
      titleKey: 'tour.ops.chapter.welcome',
      steps: [step('/ops', 'ops-home'), step('/ops', 'ops-queue-requests')],
    },
  ]
  return { CHAPTERS: { officer, ops, farmer: [] } }
})

const { TourProvider } = await import('@/app/tour/TourProvider')
const { useTour } = await import('@/app/tour/tourContext')
const { CHAPTERS } = await import('@/app/tour/tourSteps')
const { hasSeenTour, markTourSeen } = await import('@/app/tour/tourState')
await import('@/i18n')

const OFFICER = '80000000-0000-4000-8000-000000000003'

type Surface = 'officer' | 'ops' | 'farmer'

beforeEach(() => {
  Object.defineProperty(Element.prototype, 'scrollIntoView', {
    configurable: true,
    value: vi.fn(),
    writable: true,
  })
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
    new DOMRect(100, 100, 160, 48),
  )
  vi.spyOn(window, 'scrollTo').mockImplementation(() => {})
  localStorage.clear()
  navigate.mockClear()
  pathname = '/officer'
  // Joyride is unmounted while the tour is not running, so a stale `joyride`
  // here would be the PREVIOUS test's props — and `emit` would talk to a
  // provider that is no longer on the page.
  joyride = {}
})

afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
})

/**
 * Stands in for the app underneath the tour.
 *
 * It renders every stop's target, because the tour will not run until the
 * element it points at is actually on the page — which is the fix for a slow
 * screen ending the tour, and which a bare harness would otherwise defeat.
 * `omit` leaves one out, for a stop whose data is not there.
 */
function Harness({ surface = 'officer', omit = [] }: { surface?: Surface; omit?: string[] }) {
  const tour = useTour()
  const ids = CHAPTERS[surface].flatMap((chapter) => chapter.steps.map((step) => step.testId))
  return (
    <>
      <button type="button" data-testid="start" onClick={() => tour.start()}>
        {String(tour.available)}
      </button>
      <button type="button" data-testid="start-register" onClick={() => tour.start('register')} />
      <button type="button" data-testid="start-people" onClick={() => tour.start('people')} />
      <button type="button" data-testid="start-tower" onClick={() => tour.start('tower')} />
      <button type="button" data-testid="start-drill" onClick={() => tour.start('drill')} />
      <button type="button" data-testid="open-menu" onClick={tour.openMenu} />
      <span data-testid="gate-open">{String(tour.gateOpen)}</span>
      {ids
        .filter((id) => !omit.includes(id))
        .map((id) => (
          <div key={id} data-testid={id} />
        ))}
    </>
  )
}

function ui(
  props: { surface?: Surface | undefined; userId?: string | null; isAdmin?: boolean; omit?: string[] } = {},
) {
  const surface = 'surface' in props ? props.surface : ('officer' as const)
  const userId = 'userId' in props ? props.userId : OFFICER
  return (
    <TourProvider surface={surface} userId={userId} isAdmin={props.isAdmin}>
      <Harness surface={surface ?? 'officer'} omit={props.omit} />
    </TourProvider>
  )
}

function renderTour(props: Parameters<typeof ui>[0] = {}) {
  return render(ui(props))
}

/** Fire one Joyride event, the way the library would. */
function emit(data: Record<string, unknown>) {
  act(() => (joyride.onEvent as (d: unknown) => void)({ status: 'running', ...data }))
}

/**
 * Two different claims, deliberately.
 *
 * `mounted` is whether the tour exists at all — only ending it unmounts the
 * library, and unmounting is what removes its portal and therefore the overlay
 * that swallows clicks. `running` is whether it is drawing right now, which
 * also goes false while a screen is being crossed. It settles a microtask
 * after the render, because it waits for the stop's element.
 */
const mounted = () => screen.queryByTestId('joyride') !== null
const running = () => mounted() && joyride.run === true
const settle = () =>
  act(async () => {
    await Promise.resolve()
    await Promise.resolve()
  })

const next = () => emit({ action: 'next', type: 'step:after' })
const stepTargets = () => (joyride.steps as Array<{ target: string }>).map((s) => s.target)

/**
 * Start a chapter from the Harness and let the router arrive at its first stop,
 * which is when the tour can begin to draw.
 */
async function startAt(
  button: string,
  route: string,
  props: Parameters<typeof ui>[0] = {},
  view = renderTour(props),
) {
  await userEvent.click(screen.getByTestId(button))
  pathname = route
  view.rerender(ui(props))
  await settle()
  return view
}

/** The router has landed on `to`, and the tree has re-rendered because of it. */
function arriveAt(to: string, view: ReturnType<typeof renderTour>, props: Parameters<typeof ui>[0] = {}) {
  pathname = to
  view.rerender(ui(props))
}

/**
 * The tour runs itself once and then gets out of the way — and what it runs by
 * itself is the short introduction, not every chapter.
 */
describe('a tour on a first visit', () => {
  test('runs the welcome chapter for somebody who has not seen it', async () => {
    renderTour()
    await settle()

    expect(running()).toBe(true)
    expect(stepTargets()).toEqual([
      '[data-testid="officer-home"]',
      '[data-testid="officer-unverified"]',
    ])
  })

  test('does not run for somebody who has', () => {
    markTourSeen('officer', OFFICER)
    renderTour()
    expect(running()).toBe(false)
  })

  test('does not run before anyone is signed in', () => {
    renderTour({ userId: null })
    expect(running()).toBe(false)
  })

  // A person opening a record on a surface they do not work in gets no tour —
  // there is nothing sensible to say about a surface they are passing through.
  test('is unavailable off a surface entirely', () => {
    renderTour({ surface: undefined })
    expect(screen.getByTestId('start')).toHaveTextContent('false')
  })
})

describe('the tour can always be asked for again', () => {
  test('even once it has been seen, and it plays every chapter', async () => {
    markTourSeen('officer', OFFICER)
    renderTour()
    expect(running()).toBe(false)

    await userEvent.click(screen.getByTestId('start'))
    await settle()

    expect(running()).toBe(true)
    expect(stepTargets()).toContain('[data-testid="person-detail"]')
    expect(stepTargets()).not.toContain('[data-testid="redeem-screen"]')
  })

  // Already on the first stop's screen, so there is nowhere to travel to and
  // the tour must not bounce the user through a navigation to stand still.
  test('and starting it returns to the first stop', async () => {
    renderTour()
    await userEvent.click(screen.getByTestId('start'))

    expect(joyride.stepIndex).toBe(0)
    expect(navigate).not.toHaveBeenCalled()
  })

  test('one chapter plays only its own stops, and goes to where they are', async () => {
    markTourSeen('officer', OFFICER)
    const view = renderTour()

    await userEvent.click(screen.getByTestId('start-register'))
    expect(navigate).toHaveBeenCalledWith({ to: '/officer/register' })

    // The router gets there, and the tour begins to draw.
    arriveAt('/officer/register', view)
    await settle()
    expect(stepTargets()).toEqual([
      '[data-testid="register-progress"]',
      '[data-testid="register-submit"]',
    ])
  })
})

describe('the menu of chapters', () => {
  test('opens from the header button and lists what this person can play', async () => {
    markTourSeen('officer', OFFICER)
    renderTour()

    await userEvent.click(screen.getByTestId('open-menu'))

    expect(screen.getByTestId('tour-menu')).toBeInTheDocument()
    expect(screen.getByTestId('tour-chapter-register')).toBeInTheDocument()
    expect(screen.getByTestId('tour-chapter-people')).toBeInTheDocument()
    expect(screen.queryByTestId('tour-chapter-authoring'), 'admin only').not.toBeInTheDocument()
  })

  test('choosing a chapter closes the menu and plays it', async () => {
    markTourSeen('officer', OFFICER)
    const view = renderTour()

    await userEvent.click(screen.getByTestId('open-menu'))
    await userEvent.click(screen.getByTestId('tour-chapter-register'))
    arriveAt('/officer/register', view)
    await settle()

    expect(screen.queryByTestId('tour-menu')).not.toBeInTheDocument()
    expect(stepTargets()).toEqual([
      '[data-testid="register-progress"]',
      '[data-testid="register-submit"]',
    ])
  })

  test('choosing the whole tour plays all of it', async () => {
    markTourSeen('officer', OFFICER)
    renderTour()

    await userEvent.click(screen.getByTestId('open-menu'))
    await userEvent.click(screen.getByTestId('tour-chapter-all'))
    await settle()

    expect(stepTargets()).toContain('[data-testid="register-progress"]')
    expect(stepTargets()).toContain('[data-testid="person-detail"]')
  })

  test('closing it plays nothing', async () => {
    markTourSeen('officer', OFFICER)
    renderTour()

    await userEvent.click(screen.getByTestId('open-menu'))
    await userEvent.click(screen.getByTestId('tour-menu-close'))

    expect(screen.queryByTestId('tour-menu')).not.toBeInTheDocument()
    expect(running()).toBe(false)
  })
})

describe('who is shown what', () => {
  test('an admin-only chapter is played for an admin and not for anyone else', async () => {
    markTourSeen('officer', OFFICER)
    const view = renderTour()
    await userEvent.click(screen.getByTestId('start'))
    await settle()
    expect(stepTargets()).not.toContain('[data-testid="redeem-screen"]')
    view.unmount()

    markTourSeen('officer', OFFICER)
    renderTour({ isAdmin: true })
    await userEvent.click(screen.getByTestId('start'))
    await settle()
    expect(stepTargets()).toContain('[data-testid="redeem-screen"]')
  })
})

describe('what the tour hands to the library', () => {
  test('one step per stop, targeted by test id and already translated', async () => {
    renderTour()
    await settle()
    const steps = joyride.steps as Array<{ target: string; title: unknown; content: unknown }>

    expect(steps).toHaveLength(2)
    expect(steps[0].title).toBe('Welcome to Ruaha 360')
    expect(String(steps[0].content)).not.toMatch(/^tour\./)
  })

  // Played as a whole, the bubble says which chapter it is in. Played on its
  // own the chapter is the one the person just chose, and saying so is noise.
  test('names the chapter only when several are played together', async () => {
    markTourSeen('officer', OFFICER)
    renderTour()

    await userEvent.click(screen.getByTestId('start'))
    await settle()
    const whole = joyride.steps as Array<{ data: { chapterKey?: string } }>
    expect(whole[0].data.chapterKey).toBe('tour.officer.chapter.welcome')
    expect(whole[2].data.chapterKey).toBe('tour.officer.chapter.register')
  })

  test('and not when one chapter is played', async () => {
    markTourSeen('officer', OFFICER)
    await startAt('start-register', '/officer/register')
    const one = joyride.steps as Array<{ data: { chapterKey?: string } }>
    expect(one[0].data.chapterKey).toBeUndefined()
  })

  test('carries whether a stop is one to try', async () => {
    markTourSeen('officer', OFFICER)
    await startAt('start-tower', '/officer/verify')
    const steps = joyride.steps as Array<{ data: { tryIt?: boolean } }>
    expect(steps[0].data.tryIt).toBe(true)
    expect(steps[1].data.tryIt).toBeFalsy()
  })

  /**
   * The tour explains one-way actions and never performs them — and the library
   * lets a click through the spotlight to the highlighted element unless told
   * otherwise. Lit up on an explain-only stop, a Submit button would really
   * submit. Only a stop the person is invited to use lets clicks through.
   */
  test('lets a click through the spotlight only on a stop to try', async () => {
    markTourSeen('officer', OFFICER)
    await startAt('start-tower', '/officer/verify')
    const steps = joyride.steps as Array<{ blockTargetInteraction: boolean }>

    expect(steps[0].blockTargetInteraction, 'a stop to try').toBe(false)
    expect(steps[1].blockTargetInteraction, 'an explain-only stop').toBe(true)
  })

  /**
   * The tour dims the page — the only thing in this design that does — and it
   * dims it with a token, in brand navy, so the dimmed page still reads as
   * this product. And no beacon: a pulsing dot is an animation, and this
   * design has none.
   */
  test('dims with a token and opens without a beacon', async () => {
    renderTour()
    await settle()
    const options = joyride.options as Record<string, unknown>

    expect(options.overlayColor).toBe('var(--scrim)')
    expect(options.skipBeacon).toBe(true)
    expect(JSON.stringify(joyride.options)).not.toMatch(/shadow/i)
  })

  // Escape would close the step, which in this tour means "go on" — and on a
  // stop that opens a row, going on without opening it lands on a screen that
  // was never opened. Skip is the way out, and it is on the bubble.
  test('leaves Escape alone', async () => {
    renderTour()
    await settle()
    expect((joyride.options as Record<string, unknown>).dismissKeyAction).toBe(false)
  })
})

/**
 * Every way a tour can END, because getting this wrong is not a cosmetic bug.
 *
 * Joyride's overlay is a full-screen `<path fill="var(--scrim)">` with
 * `pointer-events: auto`, and it renders for the `ready` and `complete`
 * lifecycles while the TOOLTIP renders only for `tooltip`. So a tour that
 * stops advancing without stopping leaves a navy sheet over the whole page
 * that swallows every click, with no bubble left to explain it — reported from
 * the demo as "the screen stays blue and I cannot click anything", on desktop
 * and mobile, escapable only by reloading into the same trap.
 *
 * The trap is specific to CONTROLLED mode: the library discards its own index
 * patches when `stepIndex` is supplied, so both of its `→ FINISHED`
 * transitions (each needing `index >= size`) are unreachable and `STATUS`
 * never leaves `running` on its own. Ending is entirely this component's job.
 */
describe('ending the tour', () => {
  test.each([
    ['finished', 'finished'],
    ['skipped', 'skipped'],
  ])('a %s tour is not offered again', async (_name, status) => {
    renderTour()
    await settle()
    emit({ status, action: 'next', index: 1, type: 'tour:status' })

    expect(running()).toBe(false)
    expect(hasSeenTour('officer', OFFICER)).toBe(true)
  })

  /**
   * The last stop's Next. `STATUS.FINISHED` never arrives in controlled mode,
   * so the end has to be counted rather than announced.
   */
  test('the last stop ends it, and remembers', async () => {
    renderTour()
    await settle()
    for (let i = 0; i < 2; i += 1) {
      emit({ action: 'next', index: i, type: 'step:after' })
      await settle()
    }

    expect(mounted()).toBe(false)
    expect(hasSeenTour('officer', OFFICER)).toBe(true)
  })

  /**
   * And it is counted from OUR position, not from the number the library
   * reports: Joyride moves its own index on before the prop it was given
   * catches up, so an event can claim a stop ahead of the one on screen. Ending
   * on that number ends the tour a step early — which is exactly what killed
   * the ops tour between Demand and the Tower.
   */
  test('an index from the library ahead of ours does not end it early', async () => {
    renderTour()
    await settle()
    emit({ action: 'next', index: 9, type: 'step:after' })
    await settle()

    expect(mounted()).toBe(true)
    expect(joyride.stepIndex).toBe(1)
  })

  /**
   * The library's own "target not found" is ignored, and that is the fix
   * rather than an oversight.
   *
   * It is raised the instant a lifecycle changes with the target absent, which
   * every route crossing produces — it is not the claim "this element is not
   * coming". Advancing on it raced the farmer tour from stop 1 to stop 5 with
   * nobody touching it; ending on it strands a tour whose screen is merely
   * still loading. `useTourTarget` owns that decision, with a deadline.
   */
  test('the library saying "target not found" is not taken as the end', async () => {
    renderTour()
    await settle()
    emit({ action: 'next', type: 'error:target_not_found' })

    expect(running()).toBe(true)
    expect(hasSeenTour('officer', OFFICER)).toBe(false)
  })

  // A stop whose anchor never arrives ends the tour rather than sitting behind
  // a scrim — the fail-safe that every other stop relies on.
  test('a stop that never turns up ends the tour', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    renderTour({ omit: ['officer-unverified'] })
    await settle()
    next()
    await settle()

    await act(async () => {
      vi.advanceTimersByTime(10_500)
    })

    expect(mounted()).toBe(false)
    expect(hasSeenTour('officer', OFFICER)).toBe(true)
  })
})

/**
 * Crossing a screen is the other half of the same problem.
 *
 * The stop's element does not exist until the router has been there, and
 * Joyride reports a missing target immediately rather than after
 * `targetWaitTimeout`. So the run is held closed until the route matches —
 * derived from the pathname rather than restored by an effect, so there is no
 * window in which the library is looking for something that cannot be there.
 */
describe('crossing to another screen', () => {
  async function playAll() {
    markTourSeen('officer', OFFICER)
    const view = renderTour()
    await userEvent.click(screen.getByTestId('start'))
    await settle()
    return view
  }

  test('travels to the stop that lives elsewhere', async () => {
    await playAll()

    // Stop 2 is the last one on /officer, so nothing moves yet.
    next()
    await settle()
    expect(navigate).not.toHaveBeenCalled()

    // Stop 3 is not.
    next()
    expect(navigate).toHaveBeenCalledWith({ to: '/officer/register' })
  })

  /**
   * A crossing moves the tour on; it does not end it. Joyride itself is kept
   * unmounted while the route and target are being prepared, so its overlay
   * cannot block a screen it is not ready to explain.
   */
  test('and moves on without ending, while the router is still moving', async () => {
    await playAll()
    next()
    await settle()
    next()
    await settle()

    expect(mounted()).toBe(false)
    expect(navigate).toHaveBeenLastCalledWith({ to: '/officer/register' })
  })

  test('then runs again once that screen is the screen we are on', async () => {
    const view = await playAll()
    next()
    await settle()
    next()

    arriveAt('/officer/register', view)
    await settle()

    expect(running()).toBe(true)
    expect(joyride.stepIndex).toBe(2)
  })

  test('back goes back, to the screen it came from', async () => {
    const view = await playAll()
    next()
    await settle()
    next()
    arriveAt('/officer/register', view)
    await settle()

    emit({ action: 'prev', type: 'step:after' })

    expect(navigate).toHaveBeenLastCalledWith({ to: '/officer' })
  })
})

/**
 * A stop can open a row, and the stops after it are the screen it opened. The
 * tour presses Next FOR the person by clicking the row — navigation only — and
 * then waits for the router to arrive somewhere it did not choose.
 */
describe('a stop that opens a row', () => {
  function withRow(view = renderTour()) {
    const row = document.createElement('a')
    row.dataset.testid = 'people-row'
    const clicked = vi.fn()
    row.addEventListener('click', clicked)
    document.body.append(row)
    return { view, clicked, row }
  }

  async function startPeople(props: Parameters<typeof ui>[0] = {}) {
    markTourSeen('officer', OFFICER)
    const view = renderTour(props)
    await userEvent.click(screen.getByTestId('start-people'))
    pathname = '/officer/people'
    view.rerender(ui(props))
    await settle()
    return view
  }

  afterEach(() => {
    document.querySelectorAll('[data-testid="people-row"]').forEach((el) => el.remove())
  })

  test('Next opens the row and goes on to the screen it opens', async () => {
    const view = await startPeople()
    const { clicked } = withRow(view)

    next()
    await settle()

    expect(clicked).toHaveBeenCalledTimes(1)
    // Nothing to draw until the router has arrived at the screen it opened.
    expect(mounted()).toBe(false)
    // The detail screen has a parameter; the tour cannot travel to it.
    expect(navigate).not.toHaveBeenCalledWith({ to: '/officer/people/$personId' })
  })

  test('and waits for the router to arrive rather than pushing it', async () => {
    const view = await startPeople()
    withRow(view)
    next()
    await settle()
    expect(mounted()).toBe(false)

    arriveAt('/officer/people/8f2c-01', view)
    await settle()

    expect(running()).toBe(true)
    expect(joyride.stepIndex).toBe(1)
  })

  // Nothing to open is an answer, not an error: the screens behind it cannot
  // be shown, so the chapter ends instead of waiting out a deadline.
  test('with nothing to open, the screens behind it are skipped', async () => {
    await startPeople()

    next()
    await settle()

    expect(mounted()).toBe(false)
    expect(hasSeenTour('officer', OFFICER)).toBe(true)
  })

  // The browser's history undoes a click; a fresh navigation to the list would
  // throw away whatever the click carried — a filter, a picked village.
  test('Back from what it opened undoes the click rather than navigating afresh', async () => {
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => {})
    const view = await startPeople()
    withRow(view)
    next()
    arriveAt('/officer/people/8f2c-01', view)
    await settle()

    navigate.mockClear()
    emit({ action: 'prev', type: 'step:after' })
    await settle()

    expect(back).toHaveBeenCalledTimes(1)
    expect(navigate).not.toHaveBeenCalledWith({ to: '/officer/people' })
  })

  test('and does not push the router to the list while history is still on its way', async () => {
    vi.spyOn(window.history, 'back').mockImplementation(() => {})
    const view = await startPeople()
    withRow(view)
    next()
    arriveAt('/officer/people/8f2c-01', view)
    await settle()

    navigate.mockClear()
    emit({ action: 'prev', type: 'step:after' })
    // Still on the detail screen: a rerender must not navigate.
    view.rerender(ui())
    await settle()
    expect(navigate).not.toHaveBeenCalled()

    // History lands on the list, and the stop is drawn there.
    arriveAt('/officer/people', view)
    await settle()
    expect(running()).toBe(true)
    expect(joyride.stepIndex).toBe(0)
  })

  test('two stops on the screen it opened do not leave it going between them', async () => {
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => {})
    const view = await startPeople()
    withRow(view)
    next()
    arriveAt('/officer/people/8f2c-01', view)
    await settle()
    next()
    await settle()
    expect(joyride.stepIndex).toBe(2)

    // Optional app-login is present in the harness, so this is stop 2 -> 1.
    emit({ action: 'prev', type: 'step:after' })
    await settle()

    expect(back).not.toHaveBeenCalled()
    expect(joyride.stepIndex).toBe(1)
  })
})

/**
 * Some screens are plain routes that only make sense with what the click
 * carried: the Tower's drill-downs need the village that was picked. The tour
 * is taken there and never travels there itself.
 */
describe('a stop reached only by a click, on a plain route', () => {
  afterEach(() => {
    document.querySelectorAll('[data-testid="drill-link"]').forEach((el) => el.remove())
  })

  test('is never navigated to', async () => {
    markTourSeen('officer', OFFICER)
    const view = renderTour()
    await userEvent.click(screen.getByTestId('start-drill'))
    pathname = '/officer/verify'
    view.rerender(ui())
    await settle()

    const link = document.createElement('a')
    link.dataset.testid = 'drill-link'
    document.body.append(link)
    next()
    await settle()
    expect(navigate).not.toHaveBeenCalledWith({ to: '/officer/verify/drilled' })

    arriveAt('/officer/verify/drilled', view)
    await settle()
    expect(running()).toBe(true)
    expect(joyride.stepIndex).toBe(1)
  })
})

describe('a stop whose data may not exist', () => {
  afterEach(() => {
    document.querySelectorAll('[data-testid="people-row"]').forEach((el) => el.remove())
  })

  // The second stop on the detail screen depends on data that may not be
  // there. The tour goes on to the stop after it rather than being abandoned.
  test('an optional stop that does not turn up is skipped, and the tour goes on', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    markTourSeen('officer', OFFICER)
    const props = { omit: ['app-login'] }
    const view = renderTour(props)
    act(() => screen.getByTestId('start-people').click())
    pathname = '/officer/people'
    view.rerender(ui(props))
    await settle()

    const row = document.createElement('a')
    row.dataset.testid = 'people-row'
    document.body.append(row)
    next()
    arriveAt('/officer/people/8f2c-01', view, props)
    await settle()
    expect(joyride.stepIndex).toBe(1)

    next()
    await settle()
    await act(async () => {
      vi.advanceTimersByTime(4_500)
    })
    await settle()

    // Past the missing stop, to the one after it — and still running.
    expect(running()).toBe(true)
    expect(joyride.stepIndex).toBe(3)
  })
})

describe('a stop that waits for the person', () => {
  async function startTower(props: Parameters<typeof ui>[0] = {}) {
    markTourSeen('officer', OFFICER)
    const view = renderTour(props)
    await userEvent.click(screen.getByTestId('start-tower'))
    pathname = '/officer/verify'
    view.rerender(ui(props))
    await settle()
    return view
  }

  afterEach(() => {
    document.querySelectorAll('[data-testid="verify-gate"]').forEach((el) => el.remove())
  })

  test('holds Next closed until the page shows what was asked for', async () => {
    await startTower()
    expect(screen.getByTestId('gate-open')).toHaveTextContent('false')

    await act(async () => {
      const el = document.createElement('div')
      el.dataset.testid = 'verify-gate'
      document.body.append(el)
      await Promise.resolve()
    })
    await settle()

    expect(screen.getByTestId('gate-open')).toHaveTextContent('true')
  })

  test('a stop with no gate is never closed', async () => {
    renderTour()
    await settle()
    expect(screen.getByTestId('gate-open')).toHaveTextContent('true')
  })
})

/**
 * The library steps itself when its primary button is clicked, independently of
 * the `stepIndex` it was handed. When the two disagree it waits for an element
 * belonging to ITS stop while everything here still describes ours — overlay
 * up, no bubble, and the watchdog armed on the wrong element, so nothing
 * recovers. Seen on the production build, clicking through quickly: our state
 * said stop 2 and its said stop 3.
 */
describe('when the library steps itself', () => {
  async function playAll() {
    markTourSeen('officer', OFFICER)
    const view = renderTour()
    await userEvent.click(screen.getByTestId('start'))
    await settle()
    return view
  }

  test('the announced stop is followed rather than argued with', async () => {
    const view = await playAll()
    expect(joyride.stepIndex).toBe(0)

    emit({ action: 'next', index: 3, type: 'step:before' })
    await settle()

    expect(mounted()).toBe(false)
    expect(navigate).toHaveBeenLastCalledWith({ to: '/officer/register' })

    arriveAt('/officer/register', view)
    await settle()

    expect(joyride.stepIndex).toBe(3)
    expect(mounted()).toBe(true)
  })

  test('and an announcement it already agrees with changes nothing', async () => {
    await playAll()

    emit({ action: 'update', index: 0, type: 'step:before' })
    await settle()

    expect(joyride.stepIndex).toBe(0)
    expect(navigate).not.toHaveBeenCalled()
  })

  // Past the end is still the end, whoever announced it.
  test('an announced stop past the last one ends the tour', async () => {
    await playAll()

    emit({ action: 'next', index: 99, type: 'step:before' })
    await settle()

    expect(mounted()).toBe(false)
    expect(hasSeenTour('officer', OFFICER)).toBe(true)
  })
})

/**
 * A finished tour belongs to the person who finished it.
 *
 * The state that says "this tour is over" used to outlive both the user and the
 * surface, so the SECOND tour of a session never opened: finish the ops tour,
 * sign in as a farmer, and there was silently no tour at all — the same for an
 * officer who also holds ops and moves between the two.
 */
describe('a second tour in the same session', () => {
  test('opens for the next person to sign in', async () => {
    const view = renderTour()
    await settle()
    emit({ status: 'skipped', action: 'skip', index: 0, type: 'tour:status' })
    await settle()
    expect(mounted()).toBe(false)

    view.rerender(
      <TourProvider surface="officer" userId="80000000-0000-4000-8000-000000000004">
        <Harness />
      </TourProvider>,
    )
    await settle()

    expect(mounted()).toBe(true)
    expect(joyride.stepIndex).toBe(0)
  })

  test('and for the next surface the same person opens', async () => {
    const view = renderTour()
    await settle()
    emit({ status: 'skipped', action: 'skip', index: 0, type: 'tour:status' })
    await settle()

    pathname = '/ops'
    view.rerender(
      <TourProvider surface="ops" userId={OFFICER}>
        <Harness surface="ops" />
      </TourProvider>,
    )
    await settle()

    expect(mounted()).toBe(true)
    expect(stepTargets()).toEqual([
      '[data-testid="ops-home"]',
      '[data-testid="ops-queue-requests"]',
    ])
  })

  test('a chapter started from the menu is forgotten with the person who started it', async () => {
    markTourSeen('officer', OFFICER)
    const view = await startAt('start-register', '/officer/register')
    emit({ status: 'skipped', action: 'skip', index: 0, type: 'tour:status' })
    await settle()

    pathname = '/officer'
    view.rerender(
      <TourProvider surface="officer" userId="80000000-0000-4000-8000-000000000004">
        <Harness />
      </TourProvider>,
    )
    await settle()

    // Somebody else's first visit plays the welcome chapter, not the chapter
    // the last person happened to choose.
    expect(stepTargets()).toEqual([
      '[data-testid="officer-home"]',
      '[data-testid="officer-unverified"]',
    ])
  })
})

