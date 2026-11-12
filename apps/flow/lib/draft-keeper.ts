import { useEffect } from "react"

import { sessionStore } from "@genesy/api-client/session"
import { flowStore } from "@/lib/flow-store"
import { useActivityKey } from "@/lib/pills"

type TabStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">
const KEY = "genesy.flow.draft"

/** Keeps the unsent box for `owner` (user + workspace) in this tab; empty or no owner keeps nothing. */
export function keepDraft(storage: TabStorage, owner: string | null, text: string) {
  try {
    if (owner && text) storage.setItem(KEY, JSON.stringify({ owner, text }))
    else storage.removeItem(KEY)
  } catch {
    /* Tab storage may be disabled. */
  }
}

/** The kept draft if it is `owner`'s; another owner's draft is dropped. */
export function keptDraft(storage: TabStorage, owner: string): string {
  try {
    const kept = JSON.parse(storage.getItem(KEY) ?? "null") as unknown
    if (!kept || typeof kept !== "object") return ""
    const { owner: whose, text } = kept as Record<string, unknown>
    if (whose === owner && typeof text === "string") return text
    storage.removeItem(KEY)
  } catch {
    /* Unreadable or denied: nothing to give back. */
  }
  return ""
}

/**
 * P8-1: a half-typed message survives an expired login and comes back to the same person after
 * logging in again. Logging out by choice, sending, or another person's login drops it.
 */
export function useDraftKeeper() {
  const owner = useActivityKey()
  useEffect(() => {
    if (!owner) return
    // Typed before we knew whose it is (a slow device, /me still loading): keep it now (F2).
    const typed = flowStore.getState().draft
    if (typed) keepDraft(window.sessionStorage, owner, typed)
    else {
      const kept = keptDraft(window.sessionStorage, owner)
      if (kept) flowStore.getState().setDraft(kept)
    }
    return flowStore.subscribe((now, before) => {
      // Signed out: the shell's reset to an empty box is not the person clearing it.
      if (now.draft !== before.draft && sessionStore.getState().token)
        keepDraft(window.sessionStorage, owner, now.draft)
    })
  }, [owner])
  useEffect(
    () =>
      sessionStore.subscribe((now, before) => {
        if (before.token && !now.token && !now.expired)
          keepDraft(window.sessionStorage, null, "")
      }),
    []
  )
}
