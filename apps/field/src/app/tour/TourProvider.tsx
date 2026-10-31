import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useLocation, useNavigate } from '@tanstack/react-router'
import { ACTIONS, EVENTS, Joyride, STATUS, type EventData, type Step } from 'react-joyride'
import { useTranslation } from 'react-i18next'

import type { Surface } from '@/app/membership'
import { TourContext } from '@/app/tour/tourContext'
import {
  ALL_CHAPTERS,
  WELCOME_CHAPTER,
  chaptersFor,
  indexAfterSkip,
  isArrivalOnly,
  planFor,
  routeMatches,
} from '@/app/tour/tourPlan'
import { CHAPTERS } from '@/app/tour/tourSteps'
import { hasSeenTour, markTourSeen } from '@/app/tour/tourState'
import { TourMenu } from '@/app/tour/TourMenu'
import { TourTooltip } from '@/app/tour/TourTooltip'
import { useTourGate } from '@/app/tour/useTourGate'
import { useTourStall } from '@/app/tour/useTourStall'
import { useTourTarget } from '@/app/tour/useTourTarget'
import { isDemoData } from '@/lib/dataMode'
import { resourcesEnabled } from '@/lib/features'

/** How long a stop that depends on data waits for it before it is skipped. */
const OPTIONAL_WAIT_MS = 4_000

/**
 * The guided tour, one per surface, in chapters.
 *
 * Runs its short welcome chapter once on a first visit and then stays out of the
 * way; the header keeps a button that opens a menu of chapters, so it can always
 * be asked for again — and asked for at the module in front of you. What counts
 * as "seen" lives in `tourState.ts` — a device convenience, not a column.
 *
 * Joyride is driven in CONTROLLED mode because the tours cross routes: a stop
 * on `/officer/verify` cannot be shown until the router has been there. So the
 * run is held closed at a boundary, the route changes, and the stop resumes
 * once its screen is actually mounted. The alternative — one tour per screen —
 * would describe six screens without ever explaining the job they add up to.
 *
 * Controlled mode also means the library never ENDS a tour by itself: it
 * discards its own index patches when `stepIndex` is supplied, so both of its
 * `→ FINISHED` transitions are unreachable and `status` stays `running`. Every
 * way out is therefore this component's responsibility, and getting one wrong
 * is not cosmetic — the overlay renders for lifecycles the tooltip does not,
 * so a tour that stops advancing without stopping leaves a navy sheet over the
 * page that swallows every click.
 */
export function TourProvider({
  surface,
  userId,
  isAdmin = false,
  children,
}: {
  surface: Surface | undefined
  userId: string | null | undefined
  /** Whether this person holds an admin membership — the ops surface is shared. */
  isAdmin?: boolean
  children: ReactNode
}) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { pathname } = useLocation()

  const chapters = useMemo(() => (surface ? CHAPTERS[surface] : []), [surface])
  const context = useMemo(
    () => ({ isAdmin, isDemo: isDemoData, resources: resourcesEnabled }),
    [isAdmin],
  )
  const offered = useMemo(() => chaptersFor(chapters, context), [chapters, context])
  const available = offered.length > 0

  /**
   * `null` means nobody has touched the tour on this render pass, and the
   * first visit decides. Deriving it rather than starting the tour from an
   * effect keeps the "should this run" answer in one expression — and an
   * effect that sets state on mount is a cascading render for something the
   * props already know.
   */
  const [held, setHeld] = useState<{
    who: string
    running: boolean
    index: number
    chapter: string
  } | null>(null)
  const [menuOpen, setMenuOpen] = useState(false)

  /**
   * Whose tour this is. A held decision belongs to one person on one surface
   * and must not outlive either.
   *
   * Without this the state is sticky in a way that only shows up on a second
   * tour: finishing the ops tour leaves `running: false` behind, and signing in
   * as a farmer — or an officer opening the ops surface they also hold — then
   * gets no tour at all, silently, for the rest of the session.
   */
  const who = `${surface ?? ''}:${userId ?? ''}`
  const mine = held?.who === who ? held : null
  const chapterId = mine ? mine.chapter : WELCOME_CHAPTER
  const plan = useMemo(() => planFor(chapters, chapterId, context), [chapters, chapterId, context])
  const firstVisit = plan.length > 0 && Boolean(surface) && !hasSeenTour(surface!, userId)

  const active = mine ? mine.running : firstVisit
  const stepIndex = mine?.index ?? 0

  const stop = useCallback(() => {
    setHeld({ who, running: false, index: 0, chapter: chapterId })
    if (surface) markTourSeen(surface, userId)
  }, [who, chapterId, surface, userId])

  /**
   * Which stop we are on, readable from an event handler.
   *
   * Deliberately NOT the `index` the library reports. Joyride moves its own
   * index on before the prop it was given catches up, so an event can arrive
   * claiming a stop ahead of the one on screen — and a tour that decides "this
   * was the last one" from that number ends a step early, which is how the ops
   * tour kept dying between Demand and the Tower. The count is ours.
   */
  const at = useRef(stepIndex)
  useEffect(() => {
    at.current = stepIndex
  }, [stepIndex])

  /**
   * The route follows the stop, declaratively.
   *
   * Navigating from inside the event handler looked simpler and was not: the
   * library raises `step:after` from its own effect, so the call landed in the
   * middle of a commit and the router intermittently dropped it — reliably
   * enough to strand the ops tour between Demand and the Tower, which is the
   * screen the report was about. Stated as "the URL should be the stop's
   * route", it is the router's job to get there and this cannot half-happen.
   *
   * A route with a parameter is never travelled to: it is a request or a person
   * the tour did not choose, reached by the stop before it opening a row.
   */
  const here = active ? plan[stepIndex] : undefined
  const wanted = here && !isArrivalOnly(here) ? here.route : undefined
  /**
   * Set while Back is undoing a click by way of the browser's history. The stop
   * we are returning to has a plain route, and navigating to it would replace
   * whatever the click carried — the Tower's village, a list's filters — with a
   * bare URL. So the route is left alone until history has landed there.
   */
  const backing = useRef(false)
  useEffect(() => {
    if (backing.current) {
      if (!here || routeMatches(here.route, pathname)) backing.current = false
      else return
    }
    if (!wanted || wanted === pathname) return
    void navigate({ to: wanted })
  }, [here, wanted, pathname, navigate])

  const goTo = useCallback(
    (index: number) => {
      const step = plan[index]
      // Past the end, or before the start. Returning silently here is what
      // left the library running with no stop to show: a full-screen scrim
      // and no bubble, escapable only by reloading into the same trap.
      if (!step) {
        stop()
        return
      }
      setHeld({ who, running: true, index, chapter: chapterId })
    },
    [plan, who, chapterId, stop],
  )

  /**
   * Joyride's own animated scroll has stalled in a production bundle, so it
   * remains disabled. Prepare every target ourselves instead: wait for its
   * route and content, instant-scroll it into view, and only then let the
   * library mount the sheet that blocks the page.
   *
   * A stop marked `optional` depends on data that may not exist, so it waits
   * less and is skipped instead of ending the tour. Everything else that never
   * turns up still ends it: that deadline is the fail-safe.
   */
  const currentStep = active ? plan[stepIndex] : undefined
  const selector = currentStep ? `[data-testid="${currentStep.testId}"]` : null
  const targetKey = currentStep ? `${who}:${chapterId}:${stepIndex}:${currentStep.testId}` : null
  const onMissing = useCallback(() => {
    if (currentStep?.optional) goTo(indexAfterSkip(plan, stepIndex))
    else stop()
  }, [currentStep, goTo, plan, stepIndex, stop])
  const targetReady = useTourTarget(
    targetKey,
    selector,
    Boolean(currentStep) && routeMatches(currentStep!.route, pathname),
    onMissing,
    currentStep?.optional ? OPTIONAL_WAIT_MS : undefined,
  )
  const drawing = active && targetReady

  /** A mounted tour whose bubble is not genuinely visible is allowed to fail safe. */
  useTourStall(drawing, stop)

  /** A stop that asks the person to do something holds Next until it is done. */
  const gateOpen = useTourGate(
    drawing ? targetKey : null,
    drawing && currentStep?.gate ? `[data-testid="${currentStep.gate}"]` : null,
  )

  const start = useCallback(
    (chapter: string = ALL_CHAPTERS) => {
      if (planFor(chapters, chapter, context).length === 0) return
      // The effect above takes it to the first stop's screen.
      setHeld({ who, running: true, index: 0, chapter })
    },
    [chapters, context, who],
  )

  const openMenu = useCallback(() => setMenuOpen(true), [])
  const closeMenu = useCallback(() => setMenuOpen(false), [])
  const pick = useCallback(
    (chapter: string) => {
      setMenuOpen(false)
      start(chapter)
    },
    [start],
  )

  const onEvent = useCallback(
    ({ action, index, status, type }: EventData) => {
      if (status === STATUS.FINISHED || status === STATUS.SKIPPED) {
        stop()
        return
      }

      /**
       * `TARGET_NOT_FOUND` is deliberately ignored.
       *
       * The library raises it the moment a lifecycle changes with the target
       * absent, which is not the same claim as "this element is not coming" —
       * every route crossing produces that instant. Acting on it is what raced
       * the farmer tour from stop one to stop five with nobody touching it,
       * and ending on it strands a tour whose screen was merely still loading.
       *
       * `useTourTarget` answers the real question: it waits for the element and
       * says so if it never arrives. One source for that decision, and it is
       * ours.
       */

      /**
       * The library announces the stop it is about to show, and that is the
       * one place the two counters can be reconciled.
       *
       * Joyride steps itself when the primary button is clicked, independently
       * of the `stepIndex` it was given. If the two ever disagree — and on a
       * production build, clicking through quickly, they did — it waits for an
       * element belonging to ITS stop while everything here still describes
       * ours: the overlay stays up, no bubble is drawn, and the watchdog is
       * watching the wrong element, so nothing recovers. Following the number
       * it announces makes that divergence self-healing rather than terminal.
       */
      if (type === EVENTS.STEP_BEFORE && typeof index === 'number' && index !== at.current) {
        goTo(index)
        return
      }

      if (type === EVENTS.STEP_AFTER) {
        const atStep = plan[at.current]

        // A stop that opens something: Next presses it. Only navigation is ever
        // pressed — the selector names a row or a link, never a button that
        // writes. Nothing to press means the screens behind it cannot be shown.
        if (action === ACTIONS.NEXT && atStep?.open) {
          const target = document.querySelector<HTMLElement>(atStep.open)
          if (!target) {
            goTo(indexAfterSkip(plan, at.current))
            return
          }
          target.click()
          goTo(at.current + 1)
          return
        }

        // Back out of a screen the tour was taken to by a click: the browser's
        // history undoes the click, which a fresh navigation would not.
        const before = plan[at.current - 1]
        if (action === ACTIONS.PREV && atStep && before && isArrivalOnly(atStep) && atStep.route !== before.route) {
          backing.current = true
          window.history.back()
          goTo(at.current - 1)
          return
        }

        // One step from where WE are, not from where the library says it is.
        // `goTo` ends the tour when that lands past the last stop, which is
        // also the only thing that ends it: `STATUS.FINISHED` never arrives in
        // controlled mode, because the library discards its own index patches
        // when `stepIndex` is supplied and both of its `→ FINISHED`
        // transitions require an index it therefore never reaches.
        goTo(at.current + (action === ACTIONS.PREV ? -1 : 1))
      }
    },
    [goTo, plan, stop],
  )

  const steps: Step[] = useMemo(
    () =>
      plan.map((step) => ({
        // The test id is already a contract this repo keeps, so the tour
        // inherits it rather than inventing selectors of its own.
        target: `[data-testid="${step.testId}"]`,
        title: t(step.titleKey),
        content: t(step.bodyKey),
        placement: step.placement ?? 'auto',
        // The library lets a click through the spotlight to the highlighted
        // element by default. On a stop that only explains, that would let a
        // lit-up Submit button really submit — so only a stop to try lets one in.
        blockTargetInteraction: !step.tryIt,
        // Which chapter a stop is in is only worth saying when several are
        // played together; on its own, it is the one the person just chose.
        data: {
          chapterKey: chapterId === ALL_CHAPTERS ? step.chapterKey : undefined,
          tryIt: step.tryIt,
        },
      })),
    [plan, chapterId, t],
  )

  const controls = useMemo(
    () => ({ start, openMenu, available, gateOpen }),
    [start, openMenu, available, gateOpen],
  )

  return (
    <TourContext.Provider value={controls}>
      {children}
      {menuOpen && available && <TourMenu chapters={offered} onPick={pick} onClose={closeMenu} />}
      {/*
        Mounted only while an active stop is ready to draw, which makes "the
        overlay cannot outlive a usable tour" a fact about the tree rather than
        a promise about state: `TourRenderer` removes its portal on unmount, and
        the portal is what holds the sheet that swallows clicks.
      */}
      {drawing && (
        <Joyride
          key={targetKey}
          steps={steps}
          // The renderer is mounted only after its target has been positioned,
          // so it never needs to pause behind a blocking overlay.
          run
          stepIndex={stepIndex}
          onEvent={onEvent}
          continuous
          tooltipComponent={TourTooltip}
          options={{
            overlayColor: 'var(--scrim)',
            arrowColor: 'var(--paper)',
            // The spotlight is an SVG cut-out in v3, so the radius is a number
            // rather than a CSS length. 12 is --radius-card.
            spotlightRadius: 12,
            // Clicking the dimmed page is not an answer to "next or skip?".
            overlayClickAction: false,
            // Escape would close the step, which here means "go on". On a stop
            // that opens a row, going on without opening it lands on a screen
            // that was never opened. Skip is on the bubble.
            dismissKeyAction: false,
            // Clear of the fixed header, so a spotlit element is never half
            // behind it after the scroll.
            scrollOffset: 96,
            // The library does not scroll. It waits for its own scroll to
            // report finished before it draws, and on a production build that
            // report did not always arrive: the tour sat behind its overlay
            // with no bubble, permanently. `useTourTarget` does the necessary
            // instant positioning before this renderer is mounted instead.
            skipScroll: true,
            // Target preparation already waits before mounting. Keep the
            // library's matching deadline as a second defence if a prepared
            // element is replaced while Joyride is starting.
            targetWaitTimeout: 10_000,
            // No beacon. A tour that starts by asking you to find a pulsing
            // dot has added a puzzle before its first sentence — and a pulse
            // is an animation, which this design does not have.
            skipBeacon: true,
            zIndex: 40,
          }}
        />
      )}
    </TourContext.Provider>
  )
}
