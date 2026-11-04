"use client"

import { useSession } from "@genesy/api-client/auth"
import { useUsersMe } from "@genesy/api-client/endpoints/users"

/**
 * Whether `/me` grants `name` in the current workspace, to hide controls the API would refuse
 * (permissions.md: the name, `workspace_admin`, or the creator). False until `/me` answers.
 * Only a hint: the API checks every call and its 403 is still shown.
 */
export function useCan(name: string) {
  const workspaceId = useSession((s) => s.workspaceId)
  const granted = useUsersMe().data?.data.user.permissions.find(
    (p) => p.workspace === workspaceId
  )?.permissions
  return Boolean(
    granted?.some((p) =>
      [name, "workspace_admin", "workspace_owner"].includes(p)
    )
  )
}
