"use client"

import { useSession } from "./auth"
import { useUsersMe } from "./generated/users/users"

/** `may(name)` as useCan, for many names at once; `read` once `/me` has answered. */
export function useRights() {
  const workspaceId = useSession((s) => s.workspaceId)
  // Signed out, /me can only be refused: the guard redirects before anyone reads the rights.
  const token = useSession((s) => s.token)
  const me = useUsersMe({ query: { enabled: Boolean(token) } })
  const granted = me.data?.data.user.permissions.find(
    (p) => p.workspace === workspaceId
  )?.permissions
  return {
    read: me.isSuccess,
    may: (name: string) =>
      Boolean(
        granted?.some((p) =>
          [name, "workspace_admin", "workspace_owner"].includes(p)
        )
      ),
  }
}
