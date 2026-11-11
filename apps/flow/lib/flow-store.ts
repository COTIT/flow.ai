import { useStore } from "zustand"
import { createStore } from "zustand/vanilla"

import type { VisibleComponent } from "@genesy/voice/lib/scene-context"

import type { OrbMode } from "./orb-geometry"

type FlowState = {
  /** Unsent dock text: kept across navigation. */
  draft: string
  setDraft: (draft: string) => void
  /** Home shows the conversation around a larger orb. */
  immersive: boolean
  setImmersive: (immersive: boolean) => void
  /** Home shows the result scene (results and the action to review). */
  resultOpen: boolean
  setResultOpen: (open: boolean) => void
  /** The result to bring into view when the result scene opens. */
  resultFocus: string | null
  /** The person ended the last conversation here; cleared when a new one starts. */
  endedByUser: boolean
  started: () => void
  ended: () => void
  /** Measured places the one orb moves between. */
  slots: Partial<Record<OrbMode, HTMLElement>>
  setSlot: (name: OrbMode, element: HTMLElement | null) => void
  /** The record a scene has selected (ids only), for the agent's context. */
  selected: { type: string; ids: string[] } | null
  setSelected: (selected: { type: string; ids: string[] } | null) => void
  /** What the open scene shows (ids and types only), for the agent's context. */
  visible: VisibleComponent[] | null
  /** The phone's conversation sheet is raised (the orb moves into it). */
  sheetRaised: boolean
}

/** Visual state of the shell; the conversation itself lives in the voice provider. */
export const flowStore = createStore<FlowState>()((set) => ({
  draft: "",
  setDraft: (draft) => set({ draft }),
  immersive: false,
  setImmersive: (immersive) => set({ immersive }),
  resultOpen: false,
  setResultOpen: (resultOpen) => set({ resultOpen }),
  resultFocus: null,
  endedByUser: false,
  started: () => set({ endedByUser: false, immersive: true }),
  ended: () => set({ endedByUser: true, immersive: false, resultOpen: false }),
  slots: {},
  setSlot: (name, element) =>
    set((state) => {
      if (element) return { slots: { ...state.slots, [name]: element } }
      if (!state.slots[name]) return state
      const slots = { ...state.slots }
      delete slots[name]
      return { slots }
    }),
  selected: null,
  setSelected: (selected) => set({ selected }),
  visible: null,
  sheetRaised: false,
}))

export const useFlow = <T>(pick: (state: FlowState) => T) =>
  useStore(flowStore, pick)
