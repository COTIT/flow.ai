"use client"

import {
  Component,
  lazy,
  Suspense,
  useSyncExternalStore,
  type ComponentType,
  type ReactNode,
} from "react"

const REDUCE = "(prefers-reduced-motion: reduce)"

/** False on the server and under reduced motion, so nothing animated is rendered or fetched there. */
export function useMotionAllowed() {
  return useSyncExternalStore(
    (onChange) => {
      const query = matchMedia(REDUCE)
      query.addEventListener("change", onChange)
      return () => query.removeEventListener("change", onChange)
    },
    () => !matchMedia(REDUCE).matches,
    () => false
  )
}

/** If the animated piece fails to load or render, the still version stays. */
class KeepPlain extends Component<
  { plain: ReactNode; children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  render() {
    return this.state.failed ? this.props.plain : this.props.children
  }
}

/**
 * An animation that never costs the first load: its code is fetched in its own chunk, on the
 * client only, and only when the user allows motion. Until then (and on failure) the finished,
 * still version shows, so the page is complete without it. Make `plain` the same size as the
 * animated version so the swap causes no layout shift.
 *
 * Create islands once, at module level:
 *   const CountUp = island(() => import("./rb/CountUp"))
 *   <CountUp to={12} plain={<>12</>} />
 *
 * The animated piece is wrapped in `[data-motion]`; e2e checks that none exists under reduced motion.
 */
export function island<P extends object>(
  load: () => Promise<{ default: ComponentType<P> }>
) {
  const Animated = lazy(load)
  function Island({ plain, ...props }: P & { plain: ReactNode }) {
    if (!useMotionAllowed()) return <>{plain}</>
    return (
      <KeepPlain plain={plain}>
        <Suspense fallback={plain}>
          <span data-motion="" style={{ display: "contents" }}>
            <Animated {...(props as unknown as P)} />
          </span>
        </Suspense>
      </KeepPlain>
    )
  }
  return Island
}
