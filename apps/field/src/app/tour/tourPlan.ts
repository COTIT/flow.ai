import type { Chapter, TourStep } from '@/app/tour/tourTypes'

/** Runs on a first visit: the short introduction, not the whole tour. */
export const WELCOME_CHAPTER = 'welcome'

/** Every chapter, in order — the "play all" of the menu. */
export const ALL_CHAPTERS = 'all'

/** What decides which stops a person is shown. */
export interface PlanContext {
  isAdmin: boolean
  isDemo: boolean
  /** The resource release flag: equipment, requests and the catalogue. */
  resources: boolean
}

/** A stop, knowing which chapter it belongs to. */
export interface PlanStep extends TourStep {
  chapterId: string
  chapterKey: string
}

export function isPatternRoute(route: string): boolean {
  return route.includes('$')
}

/**
 * Whether the tour must wait to be taken to this stop rather than travel there.
 * A parameter it cannot fill in, or state that only a click carries.
 */
export function isArrivalOnly(step: Pick<TourStep, 'route' | 'arrive'>): boolean {
  return step.arrive === true || isPatternRoute(step.route)
}

/**
 * Whether the router is on this stop's screen.
 *
 * A plain route matches only itself. A pattern matches any one segment where
 * its parameter is, which is how the tour recognises a request or a person it
 * did not choose.
 */
export function routeMatches(route: string, pathname: string): boolean {
  if (!isPatternRoute(route)) return route === pathname
  const source = route
    .split('/')
    .map((part) => (part.startsWith('$') ? '[^/]+' : part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))
    .join('/')
  return new RegExp(`^${source}$`).test(pathname)
}

function stepShown(step: TourStep, context: PlanContext): boolean {
  if (step.audience === 'admin' && !context.isAdmin) return false
  if (step.demoOnly && !context.isDemo) return false
  if (step.feature === 'resources' && !context.resources) return false
  return true
}

function chapterShown(chapter: Chapter, context: PlanContext): boolean {
  if (chapter.audience === 'admin' && !context.isAdmin) return false
  if (chapter.demoOnly && !context.isDemo) return false
  if (chapter.feature === 'resources' && !context.resources) return false
  return true
}

/** The chapters this person is offered, each with only the stops they are shown. */
export function chaptersFor(chapters: Chapter[], context: PlanContext): Chapter[] {
  return chapters
    .filter((chapter) => chapterShown(chapter, context))
    .map((chapter) => ({ ...chapter, steps: chapter.steps.filter((step) => stepShown(step, context)) }))
    .filter((chapter) => chapter.steps.length > 0)
}

/** The stops one run plays: a single chapter, or all of them. */
export function planFor(chapters: Chapter[], chapterId: string, context: PlanContext): PlanStep[] {
  return chaptersFor(chapters, context)
    .filter((chapter) => chapterId === ALL_CHAPTERS || chapter.id === chapterId)
    .flatMap((chapter) =>
      chapter.steps.map((step) => ({ ...step, chapterId: chapter.id, chapterKey: chapter.titleKey })),
    )
}

/**
 * Where to go when a stop cannot be shown.
 *
 * A stop that opens something takes the screens it opens with it: landing on a
 * detail screen the tour never opened would only wait out its deadline.
 */
export function indexAfterSkip(plan: TourStep[], from: number): number {
  let next = from + 1
  if (plan[from]?.open) {
    while (next < plan.length && isArrivalOnly(plan[next])) next += 1
  }
  return next
}
