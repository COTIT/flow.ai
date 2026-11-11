import { nextTickDelay } from "./freshness"

type Env = {
  isHidden: () => boolean
  onVisible: (cb: () => void) => () => void
}

/** One shared timer for every "Updated … ago" line: ticks only while someone listens and the tab is visible. */
export function createFreshnessClock(env: Env) {
  const subs = new Map<() => void, () => number>()
  let timer: ReturnType<typeof setTimeout> | undefined
  let stopVisible: (() => void) | undefined

  const arm = () => {
    if (timer !== undefined || !subs.size || env.isHidden()) return
    // The youngest data decides: its text changes first.
    const delay = Math.min(
      ...[...subs.values()].map((age) => nextTickDelay(age()))
    )
    timer = setTimeout(() => {
      timer = undefined
      if (env.isHidden()) return // resumes from onVisible
      subs.forEach((_, notify) => notify())
      arm()
    }, delay)
  }
  const stop = () => {
    clearTimeout(timer)
    timer = undefined
  }

  return {
    subscribe(notify: () => void, age: () => number) {
      subs.set(notify, age)
      stopVisible ??= env.onVisible(() => {
        subs.forEach((_, n) => n())
        arm()
      })
      arm()
      return () => {
        subs.delete(notify)
        if (!subs.size) {
          stop()
          stopVisible?.()
          stopVisible = undefined
        }
      }
    },
  }
}

export const freshnessClock = createFreshnessClock({
  isHidden: () => document.hidden,
  onVisible: (cb) => {
    const on = () => document.visibilityState === "visible" && cb()
    document.addEventListener("visibilitychange", on)
    return () => document.removeEventListener("visibilitychange", on)
  },
})
