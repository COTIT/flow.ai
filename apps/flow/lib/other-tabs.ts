import { sessionStore } from "@genesy/api-client/session"

/**
 * Keeps this document's login in step with the other tabs (and frames) of this origin: the
 * browser's `storage` event fires in every other same-origin document when one writes. The
 * keyed voice provider, the /login redirect and clearOnSignOut then react to the store.
 * `cache` is emptied when another user's login replaces this one. Returns the unsubscribe.
 */
export function followOtherTabs(
  cache?: { clear: () => void },
  store = sessionStore
) {
  const name = store.persist.getOptions().name!
  /** Runs `work` with saving switched off (persist's storage is synchronous, so all of it). */
  const readOnly = (work: () => void) => {
    const storage = store.persist.getOptions().storage!
    // Never write back: this tab's view may already be stale and would wipe a newer login
    // (e.g. logout then login in tab 2), and tabs answering each other's writes would never stop.
    store.persist.setOptions({
      storage: {
        getItem: (key) => storage.getItem(key),
        setItem: () => {},
        removeItem: () => {},
      },
    })
    try {
      work()
    } finally {
      store.persist.setOptions({ storage })
    }
  }
  const onStorage = (event: StorageEvent) => {
    // key null: localStorage.clear(), which removed the session too.
    if (event.storageArea !== localStorage) return
    if (event.key !== null && event.key !== name) return
    // Read now, not event.newValue: storage may have moved on since the event was queued.
    const saved = localStorage.getItem(name)
    const token = store.getState().token
    readOnly(() => {
      // Never rehydrate a missing item: zustand 5 keeps the old state (and would save the old token back).
      if (saved === null) {
        if (token) store.getState().signOut()
        return
      }
      // A logout then a login elsewhere can arrive as one change, never passing through clearOnSignOut.
      const next = (JSON.parse(saved) as { state?: { token?: unknown } }).state
      if (token && next?.token !== token) cache?.clear()
      void store.persist.rehydrate()
    })
  }
  window.addEventListener("storage", onStorage)
  return () => window.removeEventListener("storage", onStorage)
}
