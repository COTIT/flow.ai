"use client"

import { useRights as useSharedRights } from "@genesy/api-client/rights"

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
  const rights = useSharedRights()
  return {
    read: rights.read,
    may: (name: string) =>
      rights.may(name) ||
      (name === "payment.read" &&
        (rights.may("invoice.list") || rights.may("order.list"))),
  }
}

export { rightFor } from "./routes"
