"use client"

import { useRights } from "@genesy/api-client/rights"

/**
 * Whether `/me` grants `name` in the current workspace, to hide controls the API would refuse
 * (permissions.md: the name, `workspace_admin`, or the creator). False until `/me` answers.
 * Only a hint: the API checks every call and its 403 is still shown.
 */
export function useCan(name: string) {
  return useRights().may(name)
}

export { useRights }
export { rightFor } from "./routes"
