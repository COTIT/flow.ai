import { createContext, useContext } from 'react'

export interface TourControls {
  /** Play one chapter, or the whole tour when none is named — seen before or not. */
  start: (chapterId?: string) => void
  /** Open the menu of chapters. */
  openMenu: () => void
  /** Whether there is a tour to offer here at all. */
  available: boolean
  /** False while the current stop is waiting for the person to do what it asked. */
  gateOpen: boolean
}

/**
 * Its own file so `TourProvider.tsx` exports only a component: a module that
 * exports both components and values loses fast refresh, the same reason
 * `controlStyles.ts` sits beside `controls.tsx`.
 */
export const TourContext = createContext<TourControls>({
  start: () => {},
  openMenu: () => {},
  available: false,
  gateOpen: true,
})

export function useTour(): TourControls {
  return useContext(TourContext)
}
