export const HISTORY_POINTER_KEY = "flow.conversation"
type Storage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">

/** Opaque pointer only; transcripts and credentials stay out of browser history storage. */
export function readHistoryPointer(
  storage: Storage,
  owner: string
): string | null {
  try {
    const raw = storage.getItem(HISTORY_POINTER_KEY)
    if (!raw) return null
    const value = JSON.parse(raw)
    if (
      value?.owner === owner &&
      typeof value.id === "string" &&
      value.id.length > 0 &&
      value.id.length <= 128
    )
      return value.id
    storage.removeItem(HISTORY_POINTER_KEY)
  } catch {
    try {
      storage.removeItem(HISTORY_POINTER_KEY)
    } catch {
      /* Storage may be disabled. */
    }
  }
  return null
}

export function saveHistoryPointer(
  storage: Storage,
  owner: string,
  id: string | null
) {
  try {
    if (id) storage.setItem(HISTORY_POINTER_KEY, JSON.stringify({ owner, id }))
    else storage.removeItem(HISTORY_POINTER_KEY)
  } catch {
    /* Storage may be disabled; the current conversation still works. */
  }
}
