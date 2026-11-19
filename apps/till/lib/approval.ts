import { createStore } from "zustand/vanilla"
import { useStore } from "zustand"

import type { PrivilegedSessionResource } from "@genesy/api-client/model"
import { tillStore } from "@/lib/till-store"

/** A member's approval login for refunds, returns and drawer adjustments. */
export type Approval = {
  /** The member's own bearer token. Memory only: never localStorage, cookies or disk. */
  token: string
  /** The email the member typed, shown as who approves. */
  email: string
  /** Who approves, on which register, for which cashier (from GET /till/privileged-session). */
  session: PrivilegedSessionResource
}

type ApprovalState = {
  approval: Approval | null
  approve: (a: Approval) => void
  end: () => void
}

/**
 * The approval login, kept in memory only (no `persist`): a reload, Lock or a till logout drops
 * it, and logging the member out (or revoking them) ends it on the server too.
 */
export const approvalStore = createStore<ApprovalState>()((set) => ({
  approval: null,
  approve: (approval) => set({ approval }),
  end: () => set({ approval: null }),
}))

// The cashier's session ends (Lock, expiry): the approval goes with it.
tillStore.subscribe((now, before) => {
  if (before.session && !now.session) approvalStore.getState().end()
})

export function useApproval() {
  return useStore(approvalStore, (s) => s.approval)
}

/**
 * Per-call headers of a privileged route: the member's bearer plus the till session. The
 * `X-Till-Session` header also tells the 401 handler not to lock the cashier.
 */
export function privileged(a: Pick<Approval, "token"> | null) {
  return {
    headers: {
      Authorization: `Bearer ${a?.token ?? ""}`,
      "X-Till-Session": tillStore.getState().session?.token ?? "",
    },
  }
}

/** True when the member holds `right` (read live from the server at approval). */
export const holds = (a: Approval | null, right: string) =>
  a?.session.member.permissions.includes(right) ?? false
