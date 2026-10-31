import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vitest'

import { createMemoryDraftStore, useDraft, type DraftStore } from '@/lib/drafts'

let store: DraftStore

beforeEach(() => {
  store = createMemoryDraftStore()
})

describe('useDraft', () => {
  test('reports empty when there is nothing stored', async () => {
    const { result } = renderHook(() => useDraft<{ a: number }>('register:new', store))
    await waitFor(() => expect(result.current.status).toBe('empty'))
    expect(result.current.draft).toBeUndefined()
  })

  test('restores an existing draft on mount', async () => {
    await store.set('register:abc', { given_name: 'Neema' })

    const { result } = renderHook(() => useDraft<{ given_name: string }>('register:abc', store))
    await waitFor(() => expect(result.current.draft).toEqual({ given_name: 'Neema' }))
    expect(result.current.status).toBe('dirty')
  })

  // The rule that matters: saving locally must NOT report success. An unsaved
  // write has to look unsaved, so the status stays dirty and the badge stays up.
  test('a local save leaves the status dirty, never saved', async () => {
    const { result } = renderHook(() => useDraft<{ given_name: string }>('register:abc', store))
    await waitFor(() => expect(result.current.status).toBe('empty'))

    await act(async () => {
      await result.current.save({ given_name: 'Neema' })
    })

    expect(result.current.status).toBe('dirty')
    expect(result.current.status).not.toBe('saved')
    await expect(store.get('register:abc')).resolves.toEqual({ given_name: 'Neema' })
  })

  // Cleared only after a confirmed server write, which is what 'saved' means.
  test('clear empties the draft and reports saved', async () => {
    await store.set('register:abc', { given_name: 'Neema' })
    const { result } = renderHook(() => useDraft<{ given_name: string }>('register:abc', store))
    await waitFor(() => expect(result.current.draft).toBeDefined())

    await act(async () => {
      await result.current.clear()
    })

    expect(result.current.status).toBe('saved')
    expect(result.current.draft).toBeUndefined()
    await expect(store.get('register:abc')).resolves.toBeUndefined()
  })

  test('a broken store does not take the form down', async () => {
    const broken: DraftStore = {
      get: async () => {
        throw new Error('storage disabled')
      },
      set: async () => {
        throw new Error('quota exceeded')
      },
      clear: async () => {},
    }

    const { result } = renderHook(() => useDraft<{ a: number }>('register:abc', broken))
    await waitFor(() => expect(result.current.status).toBe('empty'))

    await act(async () => {
      await result.current.save({ a: 1 })
    })

    // The value is still in memory for the live form, even though it did not
    // reach storage.
    expect(result.current.draft).toEqual({ a: 1 })
  })
})

/**
 * QA #22. A draft of the wrong shape was restored verbatim: First name
 * rendered as the literal `[object Object]`, Family name as `array`, the "not
 * yet submitted" badge appeared, and the form would have submitted those
 * values.
 *
 * The realistic route is not tampering. It is **a draft written by an older
 * deployment of the form** — rename or retype any field in `RegisterForm` and
 * every draft in the field becomes this, on the phones that were mid-
 * registration when the app updated. Which is the scenario the draft feature
 * exists for.
 */
describe('useDraft with a shape guard', () => {
  const isPerson = (value: unknown): value is { given_name: string } =>
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { given_name?: unknown }).given_name === 'string'

  test('restores a draft that still matches the form', async () => {
    await store.set('register:abc', { given_name: 'Neema' })

    const { result } = renderHook(() =>
      useDraft<{ given_name: string }>('register:abc', store, isPerson),
    )

    await waitFor(() => expect(result.current.status).toBe('dirty'))
    expect(result.current.draft).toEqual({ given_name: 'Neema' })
  })

  test('discards one that does not, rather than rendering it', async () => {
    await store.set('register:abc', { given_name: { nested: true }, family_name: ['array'] })

    const { result } = renderHook(() =>
      useDraft<{ given_name: string }>('register:abc', store, isPerson),
    )

    await waitFor(() => expect(result.current.status).toBe('empty'))
    expect(result.current.draft).toBeUndefined()
  })

  // Leaving it in storage would mean discarding it again on every reload, and
  // the badge would keep promising a draft that will never be restored.
  test('and removes it from storage, so it is not re-read forever', async () => {
    await store.set('register:abc', { wrong: 'shape' })

    const { result } = renderHook(() =>
      useDraft<{ given_name: string }>('register:abc', store, isPerson),
    )
    await waitFor(() => expect(result.current.status).toBe('empty'))

    await waitFor(async () => expect(await store.get('register:abc')).toBeUndefined())
  })

  test('a primitive where an object belongs is discarded too', async () => {
    await store.set('register:abc', 'just a string')

    const { result } = renderHook(() =>
      useDraft<{ given_name: string }>('register:abc', store, isPerson),
    )

    await waitFor(() => expect(result.current.status).toBe('empty'))
  })

  // Without a guard the hook behaves exactly as before: every other caller is
  // unaffected.
  test('no guard means no checking', async () => {
    await store.set('register:abc', { wrong: 'shape' })

    const { result } = renderHook(() => useDraft<unknown>('register:abc', store))

    await waitFor(() => expect(result.current.status).toBe('dirty'))
    expect(result.current.draft).toEqual({ wrong: 'shape' })
  })
})
