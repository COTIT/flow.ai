"use client"

import { useSession } from "@genesy/api-client/auth"
import { useUsersMe } from "@genesy/api-client/endpoints/users"

/**
 * Whether `/me` grants `name` in the current workspace, to hide controls the API would refuse
 * (permissions.md: the name, `workspace_admin`, or the creator). False until `/me` answers.
 * Only a hint: the API checks every call and its 403 is still shown.
 */
export function useCan(name: string) {
  return useRights().may(name)
}

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
          [
            name,
            "workspace_admin",
            "workspace_owner",
            ...(name === "payment.read" ? ["invoice.list", "order.list"] : []),
          ].includes(p)
        )
      ),
  }
}

export { rightFor } from "./routes"
