import { useSyncExternalStore } from "react"

const QUERY = "(min-width: 1024px)"

const subscribe = (change: () => void) => {
  const list = window.matchMedia(QUERY)
  list.addEventListener("change", change)
  return () => list.removeEventListener("change", change)
}

/** Room for two panes side by side (1024 px and up); below that results open in a sheet. */
export function useWide() {
  return useSyncExternalStore(
    subscribe,
    () => window.matchMedia(QUERY).matches,
    () => false
  )
}
