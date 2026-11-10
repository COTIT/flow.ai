"use client"

import { useSession } from "./auth"
import { useUsersMe } from "./generated/users/users"

/** `may(name)` as useCan, for many names at once; `read` once `/me` has answered. */
export function useRights() {
  const workspaceId = useSession((s) => s.workspaceId)
  const me = useUsersMe()
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
