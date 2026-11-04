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
          [name, "workspace_admin", "workspace_owner"].includes(p)
        )
      ),
  }
}

/** The right each office route needs: the one the API checks on the list its page reads. */
const routeRights: [prefix: string, right: string][] = [
  ["/products", "product.list"],
  ["/stock", "stock.list"],
  ["/orders", "order.list"],
  ["/invoices", "invoice.list"],
  ["/supply/links", "workspace_admin"],
  ["/till", "cashregister.list"],
  ["/settings/app-keys", "workspace_admin"],
  ["/settings/webhooks", "workspace_admin"],
  ["/settings/invites", "invite.list"],
  ["/search", "workspace.global-search"],
]

/** The right `path` needs, or undefined for a route every member may use (batches, transfers). */
export const rightFor = (path: string) =>
  routeRights.find(([p]) => path === p || path.startsWith(`${p}/`))?.[1]
