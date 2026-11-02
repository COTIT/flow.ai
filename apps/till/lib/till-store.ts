import { createJSONStorage, persist } from "zustand/middleware"
import { createStore } from "zustand/vanilla"
import { useStore } from "zustand"

import type { TillLoginResource } from "@genesy/api-client/model"

/** This device's pairing: the workspace and the register's device key, from `POST /till/pair`. */
export type Device = {
  workspaceId: string
  deviceKey: string
  /** The register's name at pairing time: shown in the login bar, nothing else. */
  registerName?: string
}

export type TillState = {
  device: Device | null
  /** The till user's session (token, register, user), or `null` at the PIN screen. */
  session: TillLoginResource | null
  /** `false` until the saved state was read back from localStorage. */
  hydrated: boolean
  /** Why the last session ended, when it wasn't the user's own logout (for the PIN screen). */
  ended: "expired" | null
  pair: (device: Device) => void
  unpair: () => void
  signIn: (login: TillLoginResource) => void
  signOut: (reason?: "expired") => void
}

/**
 * Device pairing and till session, one per browser.
 * shortcut: the device key and token sit in localStorage like the member token; the CSP guards them.
 */
export const tillStore = createStore<TillState>()(
  persist(
    (set) => ({
      device: null,
      session: null,
      hydrated: false,
      ended: null,
      pair: (device) => set({ device, session: null }),
      unpair: () => set({ device: null, session: null }),
      signIn: (session) => set({ session, ended: null }),
      // Only a live session can expire: a wrong PIN's 401 has none to end.
      signOut: (reason) =>
        set((s) => ({
          session: null,
          ended: s.session ? (reason ?? null) : s.ended,
        })),
    }),
    {
      name: "genesy.till",
      storage: createJSONStorage(() => localStorage),
      partialize: ({ device, session }) => ({ device, session }),
      // Read storage after the first render, so server and browser HTML match.
      skipHydration: true,
      onRehydrateStorage: () => () => tillStore.setState({ hydrated: true }),
    }
  )
)

/** Reads the till state in a component and re-renders when the picked part changes. */
export function useTill<T>(pick: (s: TillState) => T): T {
  return useStore(tillStore, pick)
}

/** Per-call header for routes that take the device key instead of the till token. */
export function deviceAuth(device: Device | null) {
  return { headers: { Authorization: `Bearer ${device?.deviceKey ?? ""}` } }
}
