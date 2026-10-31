/**
 * A stop on a guided tour, as data.
 *
 * `testId` rather than a CSS selector: `data-testid` is already a contract this
 * repo keeps — `pnpm e2e` fails when one moves — so the tour inherits that
 * contract instead of inventing a parallel one out of class names, which would
 * rot on the next restyle without a single test failing.
 *
 * `route` is where the stop lives. The tour navigates there before showing it,
 * which is what lets a tour explain a whole role rather than one screen. A route
 * with a `$parameter` in it is a screen the tour cannot travel to by itself — a
 * request, a person, a survey — so it is only ever reached by the stop before it
 * opening a row (`open`).
 */
export interface TourStep {
  testId: string
  route: string
  titleKey: string
  bodyKey: string
  /** `center` parks the bubble mid-screen, for a stop that is about a whole page. */
  placement?: 'auto' | 'center' | 'top' | 'bottom' | 'left' | 'right'
  /**
   * The tour never navigates to this stop's screen itself: it is reached by the
   * stop before it opening something. A route with a `$parameter` is always
   * like this; this flag is for a plain route that only makes sense with the
   * state the click carries — the Tower's drill-downs need the village that was
   * picked, which a bare URL would lose.
   */
  arrive?: boolean
  /**
   * A CSS selector for the row or link this stop opens when the person presses
   * Next. Opening something is navigation and nothing else: a tour never clicks
   * anything that writes. The stops that follow, on a `$parameter` route, are
   * the screen it opened.
   */
  open?: string
  /**
   * The person is invited to use the highlighted part themselves — type into
   * it, pick from it, filter it. Only ever for something that changes nothing
   * that is kept, and it never leaves the screen the stop is on.
   */
  tryIt?: boolean
  /** Next stays disabled until an element with this test id is on the page. */
  gate?: string
  /**
   * The anchor depends on data that may not exist. If it does not turn up, the
   * stop is skipped rather than the whole tour being abandoned.
   */
  optional?: boolean
  /** Shown to admins only — the ops surface is shared with a role that cannot author. */
  audience?: 'admin'
  /** Names seeded demo accounts, so it means nothing anywhere else. */
  demoOnly?: boolean
  /** On a screen held back by a release flag (`@/lib/features`); dropped when it is off. */
  feature?: 'resources'
}

/** A module of the app, played on its own or as one part of the whole. */
export interface Chapter {
  id: string
  titleKey: string
  audience?: 'admin'
  demoOnly?: boolean
  feature?: 'resources'
  steps: TourStep[]
}
