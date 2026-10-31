import i18next from 'i18next'
import { useCallback, useEffect, useRef, useState } from 'react'

/**
 * Drafts and interrupted saves — business-rules §12.
 *
 * Rules this honours:
 *  - persist on change, keyed by form name plus client_ref
 *  - restore on mount; clear ONLY after a confirmed server write
 *  - a draft renders an UnsavedDraftBadge. An unsaved write must LOOK unsaved:
 *    no success toast for something that only reached local storage
 *  - retry is explicit and user-initiated
 *
 * Explicitly NOT here: no background queue, no replay engine, no conflict
 * resolution. That is offline sync, which Plan v2 defers until field testing
 * proves it necessary.
 */

export type DraftStatus = 'empty' | 'restoring' | 'dirty' | 'saved'

export interface DraftStore {
  get: (key: string) => Promise<unknown>
  set: (key: string, value: unknown) => Promise<void>
  clear: (key: string) => Promise<void>
}

/** Form name plus client_ref, so two registrations in flight cannot collide. */
export function draftKey(form: string, clientRef: string): string {
  return `${form}:${clientRef}`
}

const DB_NAME = 'ruaha360'
const STORE_NAME = 'drafts'

// One queue per key across mounted forms. A clear waits for older writes and
// subsequent saves cannot overtake it, including when a dialog remounts.
const pending = new Map<string, Promise<unknown>>()
export function queueDraft<T>(key: string, work: () => Promise<T>): Promise<T> {
  const next = (pending.get(key) ?? Promise.resolve()).catch(() => {}).then(work)
  pending.set(key, next)
  void next.finally(() => { if (pending.get(key) === next) pending.delete(key) }).catch(() => {})
  return next
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1)
    request.onupgradeneeded = () => {
      const db = request.result
      if (!db.objectStoreNames.contains(STORE_NAME)) db.createObjectStore(STORE_NAME)
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('could not open IndexedDB'))
  })
}

function tx<T>(mode: IDBTransactionMode, run: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  return openDb().then(
    (db) =>
      new Promise<T>((resolve, reject) => {
        const transaction = db.transaction(STORE_NAME, mode)
        const request = run(transaction.objectStore(STORE_NAME))
        transaction.oncomplete = () => { db.close(); resolve(request.result) }
        transaction.onabort = () => { db.close(); reject(transaction.error ?? new Error('IndexedDB transaction aborted')) }
        request.onerror = () => reject(request.error ?? new Error('IndexedDB request failed'))
      }),
  )
}

/** IndexedDB-backed store. The real implementation, used in the browser. */
export const indexedDbDraftStore: DraftStore = {
  get: (key) => tx('readonly', (s) => s.get(key)),
  set: async (key, value) => {
    await tx('readwrite', (s) => s.put(value, key))
  },
  clear: async (key) => {
    await tx('readwrite', (s) => s.delete(key))
  },
}

/** In-memory store, for tests and for environments without IndexedDB. */
export function createMemoryDraftStore(): DraftStore {
  const map = new Map<string, unknown>()
  return {
    get: async (key) => map.get(key),
    set: async (key, value) => void map.set(key, value),
    clear: async (key) => void map.delete(key),
  }
}

/**
 * A browser with storage disabled must not take the form down with it. The
 * draft is a convenience; losing it is bad, crashing is worse.
 */
export async function safeGet(store: DraftStore, key: string): Promise<unknown> {
  try {
    return await store.get(key)
  } catch {
    return undefined
  }
}

export async function safeSet(store: DraftStore, key: string, value: unknown): Promise<boolean> {
  try {
    await store.set(key, value)
    return true
  } catch {
    return false
  }
}

export interface Draft<T> {
  /** Restored draft, or undefined when there is nothing stored. */
  draft: T | undefined
  /** Persist locally. NOT a server write — status stays 'dirty'. */
  save: (value: T) => Promise<void>
  /** Call only after the server has confirmed the write. */
  clear: () => Promise<void>
  status: DraftStatus
  storageError: boolean
}

/**
 * @param isValid Optional shape guard for what comes back out of storage.
 *
 * QA #22: a draft of the wrong shape was restored verbatim — First name
 * rendering as the literal `[object Object]`, the "not yet submitted" badge
 * up, and the form ready to submit it. The realistic cause is not tampering
 * but **a draft written by an older deployment of the form**: rename or retype
 * one field and every draft in the field becomes this, on exactly the phones
 * the feature exists to protect.
 *
 * A draft that no longer matches is discarded AND removed from storage —
 * otherwise it is re-read and discarded again on every reload.
 */
export function useDraft<T>(
  key: string,
  store: DraftStore = indexedDbDraftStore,
  isValid?: (value: unknown) => value is T,
): Draft<T> {
  const storeRef = useRef(store)
  const [storageError, setStorageError] = useState(false)
  const cleared = useRef<string | null>(null)
  // Initialised from the first render's guard, then kept current in an effect
  // — writing a ref during render is what `react-hooks/refs` forbids, and the
  // initial value is already right on mount, which is when the restore runs.
  const validRef = useRef(isValid)
  useEffect(() => {
    validRef.current = isValid
  }, [isValid])

  // Key, draft and status move together so a key change cannot leave a draft
  // from the previous form on screen.
  const [state, setState] = useState<{
    key: string
    draft: T | undefined
    status: DraftStatus
  }>({ key, draft: undefined, status: 'restoring' })

  // Adjusting state during render when the key changes, rather than in an
  // effect: setting state synchronously inside an effect triggers a cascading
  // render, and the reset must be visible in the same commit as the new key.
  if (state.key !== key) {
    setState({ key, draft: undefined, status: 'restoring' })
  }

  useEffect(() => {
    let cancelled = false
    void safeGet(storeRef.current, key).then((value) => {
      if (cancelled) return

      const check = validRef.current
      if (value !== undefined && check && !check(value)) {
        // Not ours any more. Drop it from storage too, or it is re-read and
        // discarded again on every reload while the badge promises a draft
        // that will never come back.
        void storeRef.current.clear(key).catch(() => {})
        setState({ key, draft: undefined, status: 'empty' })
        return
      }

      setState({ key, draft: value as T | undefined, status: value === undefined ? 'empty' : 'dirty' })
    })
    return () => {
      cancelled = true
    }
  }, [key])

  const save = useCallback(
    async (value: T) => {
      if (cleared.current === key) return
      // Stays 'dirty': it only reached local storage. An unsaved write must
      // look unsaved, so this is never reported as saved.
      setState({ key, draft: value, status: 'dirty' })
      const saved = await queueDraft(key, () => safeSet(storeRef.current, key, value))
      setStorageError(!saved)
    },
    [key],
  )

  const clear = useCallback(async () => {
    cleared.current = key
    try {
      await queueDraft(key, () => storeRef.current.clear(key))
    } catch {
      setStorageError(true)
    }
    setState({ key, draft: undefined, status: 'saved' })
  }, [key])

  return { draft: state.draft, save, clear, status: state.status, storageError }
}

/**
 * The id collided with a row saved earlier from this draft, but with other
 * values: the user edited the form after an uncertain submit. Not a success,
 * and not a database message — the record exists, so the form clears its
 * draft and says where to look.
 */
export class EarlierVersionSavedError extends Error {
  constructor() {
    super(i18next.t('draft.earlierVersionSaved'))
    this.name = 'EarlierVersionSavedError'
  }
}

/**
 * Clears the draft once the record exists. Chained on `mutateAsync`, not on
 * `mutate`'s per-call callbacks: those are skipped once the screen unmounts,
 * and the draft would outlive a confirmed save. Other failures keep the
 * draft; they show through the mutation's own error state.
 */
export function finishDraftWhenSaved(save: Promise<unknown>, finish: () => Promise<void>): Promise<void> {
  return save.then(
    () => finish(),
    (error: unknown) => (error instanceof EarlierVersionSavedError ? finish() : undefined),
  )
}
