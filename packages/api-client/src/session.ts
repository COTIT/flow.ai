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
  /**
   * `true` when the last logout came from the backend refusing a live token (a 401), so a
   * login page can say "your session ended". Cleared by the next login; not saved.
   */
  expired: boolean
  signIn: (login: TokenResponse) => void
  selectWorkspace: (id: string) => void
  /** `"expired"` is for the 401 handler; a logout button calls it with nothing. */
  signOut: (reason?: "expired") => void
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
      expired: false,
      signIn: (login) =>
        set({
          expired: false,
          token: login.access_token,
          workspaceIds: login.workspace_ids,
          workspaceId: login.workspace_ids[0] ?? null,
        }),
      selectWorkspace: (id) => {
        if (get().workspaceIds.includes(id)) set({ workspaceId: id })
      },
      signOut: (reason) => {
        // Parallel calls can each get the 401: the later ones (token already gone) keep the mark.
        const expired =
          reason === "expired" && (get().expired || Boolean(get().token))
        set({ ...loggedOut, expired })
        // Drop the saved copy too, not just the token inside it.
        sessionStore.persist.clearStorage()
      },
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

/**
 * Empties a query cache whenever the user logs out (button or a 401), so the next
 * user never sees the previous one's data. Returns the unsubscribe function.
 */
export function clearOnSignOut(cache: { clear: () => void }) {
  return sessionStore.subscribe((now, before) => {
    if (before.token && !now.token) cache.clear()
  })
}
