import { useEffect, useRef, useState } from 'react'
import { useForm, useWatch, type DefaultValues, type Path, type PathValue, type Resolver } from 'react-hook-form'
import { z } from 'zod'
import { useSession } from '@/app/session'
import { indexedDbDraftStore, queueDraft, type DraftStore } from '@/lib/drafts'
import { newUuid } from '@/lib/ids'

// `version` is optional so drafts written before it existed still parse.
const envelope = z.object({
  clientRef: z.string().uuid(),
  values: z.record(z.string(), z.string()),
  version: z.string().nullish(),
})

export interface PersistentFormOptions {
  store?: DraftStore
  /**
   * The server record's version (e.g. its `captured_at`). A stored draft of a
   * different version is discarded on restore: the record changed since it
   * was typed, and an abandoned edit must not resurrect over newer data.
   */
  version?: string | null
}

type PersistentState = { key: string; ready: boolean; dirty: boolean; storageError: boolean; clientRef: string }

const versioned = (version: string | null | undefined) => (version == null ? {} : { version })

/** Form-only persistence; never queues or replays a server mutation. */
export function usePersistentForm<T extends Record<string, string>>(
  name: string,
  scope: string,
  defaults: T,
  resolver?: Resolver<T>,
  options: DraftStore | PersistentFormOptions = {},
) {
  const { store = indexedDbDraftStore, version } = 'get' in options ? { store: options, version: undefined } : options
  const session = useSession()
  const owner = session.data?.userId
  const key = `form:${owner ?? 'unavailable'}:${scope}:${name}`
  const form = useForm<T>({ defaultValues: defaults as DefaultValues<T>, resolver })
  const values = useWatch({ control: form.control }) as T
  const [state, setState] = useState<PersistentState>(() => ({ key, ready: !owner, dirty: false, storageError: false, clientRef: newUuid() }))
  const active = useRef(key)
  const closed = useRef(false)
  // Fields the user typed before the stored draft had been read.
  const early = useRef<Partial<T>>({})
  const defaultsRef = useRef(defaults)
  const storeRef = useRef(store)
  // Kept current in an effect, declared before the restore so it runs first.
  const versionRef = useRef(version)
  useEffect(() => { versionRef.current = version }, [version])
  const { reset, getValues, setValue } = form

  if (state.key !== key) {
    setState({ key, ready: !owner, dirty: false, storageError: false, clientRef: newUuid() })
  }

  useEffect(() => {
    active.current = key
    closed.current = false
    early.current = {}
    let cancelled = false
    if (!owner) return () => { cancelled = true }
    void queueDraft(key, async () => {
      let storageError = false
      let restored: z.infer<typeof envelope> | undefined
      try {
        const raw = await storeRef.current.get(key)
        const parsed = envelope.safeParse(raw)
        const expected = versionRef.current
        if (
          parsed.success
          && Object.keys(defaultsRef.current).every(k => typeof parsed.data.values[k] === 'string')
          && (expected == null || parsed.data.version === expected)
        ) restored = parsed.data
        else if (raw !== undefined) await storeRef.current.clear(key)
      } catch { storageError = true }
      if (cancelled) return
      const clientRef = restored?.clientRef ?? newUuid()
      if (Object.keys(early.current).length > 0) {
        // Keystrokes that beat the restore are the latest intent for THOSE
        // fields: laid over the restored draft, not in place of it. The
        // restored clientRef is kept, so an uncertain earlier submit still
        // reconciles by id.
        const merged = { ...(restored?.values ?? getValues()), ...early.current } as T
        if (restored) reset(merged as DefaultValues<T>)
        setState({ key, ready: true, dirty: true, storageError, clientRef })
        try {
          await storeRef.current.set(key, { clientRef, values: merged, ...versioned(versionRef.current) })
        } catch {
          if (!cancelled) setState(s => ({ ...s, storageError: true }))
        }
        return
      }
      reset((restored?.values ?? defaultsRef.current) as DefaultValues<T>)
      setState({ key, ready: true, dirty: !!restored, storageError, clientRef })
    })
    return () => { cancelled = true }
  }, [key, owner, reset, getValues])

  function field<K extends keyof T & string>(name: K): [T[K], (value: T[K]) => void] {
    return [values[name] ?? defaults[name], value => {
      if (state.key !== key || closed.current) return
      setValue(name as unknown as Path<T>, value as unknown as PathValue<T, Path<T>>, { shouldDirty: true, shouldValidate: true })
      if (!state.ready) {
        // Held in the form; the restore persists it once storage has answered.
        early.current = { ...early.current, [name]: value }
        setState(s => ({ ...s, dirty: true }))
        return
      }
      const snapshot = { ...getValues(), [name]: value }
      setState(s => ({ ...s, dirty: true }))
      if (!owner) return
      const clientRef = state.clientRef
      void queueDraft(key, async () => {
        if (closed.current || active.current !== key) return
        try {
          await storeRef.current.set(key, { clientRef, values: snapshot, ...versioned(versionRef.current) })
          if (active.current === key) setState(s => ({ ...s, storageError: false }))
        } catch {
          if (active.current === key) setState(s => ({ ...s, storageError: true }))
        }
      })
    }]
  }

  async function finish() {
    closed.current = true
    try { await queueDraft(key, () => storeRef.current.clear(key)) }
    catch {
      // The server write succeeded; only the local copy survived. It keeps its
      // clientRef, so resubmitting it reconciles by id instead of duplicating.
      closed.current = false
      if (active.current !== key) return
      reset(defaultsRef.current as DefaultValues<T>)
      setState({ key, ready: true, dirty: false, storageError: true, clientRef: newUuid() })
      return
    }
    closed.current = false
    if (active.current !== key) return
    reset(defaultsRef.current as DefaultValues<T>)
    setState({ key, ready: true, dirty: false, storageError: false, clientRef: newUuid() })
  }

  return { ...form, field, values, finish, ...state, ready: state.key === key && state.ready }
}
