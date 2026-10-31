import { createJSONStorage, persist } from "zustand/middleware"
import { createStore } from "zustand/vanilla"

import type { TokenResponse } from "./generated/model"

export type Session = {
  /** Bearer token, or `null` when logged out. */
  token: string | null
  /** Workspaces this login belongs to, from the login answer. */
  workspaceIds: string[]
  /** The workspace the screens show. */
  workspaceId: string | null
  /** `false` until the saved session was read back from localStorage (always `false` on the server). */
  hydrated: boolean
  signIn: (login: TokenResponse) => void
  selectWorkspace: (id: string) => void
  signOut: () => void
}

const loggedOut = { token: null, workspaceIds: [], workspaceId: null }

/**
 * One login per browser, shared by every tab (localStorage, user decision on G-auth).
 * shortcut: an XSS bug could read the token; add a CSP before the first public launch.
 */
export const sessionStore = createStore<Session>()(
  persist(
    (set, get) => ({
      ...loggedOut,
      hydrated: false,
      signIn: (login) =>
        set({
          token: login.access_token,
          workspaceIds: login.workspace_ids,
          workspaceId: login.workspace_ids[0] ?? null,
        }),
      selectWorkspace: (id) => {
        if (get().workspaceIds.includes(id)) set({ workspaceId: id })
      },
      signOut: () => set(loggedOut),
    }),
    {
      name: "genesy.session",
      storage: createJSONStorage(() => localStorage),
      partialize: ({ token, workspaceIds, workspaceId }) => ({
        token,
        workspaceIds,
        workspaceId,
      }),
      // Read storage after the first render, so server and browser HTML match.
      skipHydration: true,
      onRehydrateStorage: () => () => sessionStore.setState({ hydrated: true }),
    }
  )
)
