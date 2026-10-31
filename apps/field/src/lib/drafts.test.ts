import { beforeEach, describe, expect, test, vi } from 'vitest'

import { createMemoryDraftStore, type DraftStore } from '@/lib/drafts'

let store: DraftStore

beforeEach(() => {
  store = createMemoryDraftStore()
})

describe('draft store contract', () => {
  test('reading an unknown key yields undefined, not a throw', async () => {
    await expect(store.get('register:abc')).resolves.toBeUndefined()
  })

  test('a saved draft reads back', async () => {
    await store.set('register:abc', { given_name: 'Neema' })
    await expect(store.get('register:abc')).resolves.toEqual({ given_name: 'Neema' })
  })

  test('saving again replaces the previous value', async () => {
    await store.set('register:abc', { given_name: 'Neema' })
    await store.set('register:abc', { given_name: 'Neema', family_name: 'Mwakalinga' })
    await expect(store.get('register:abc')).resolves.toEqual({
      given_name: 'Neema',
      family_name: 'Mwakalinga',
    })
  })

  // Keyed by form name plus client_ref, so two registrations in flight cannot
  // overwrite each other.
  test('keys are independent', async () => {
    await store.set('register:one', { given_name: 'Neema' })
    await store.set('register:two', { given_name: 'Joseph' })
    await expect(store.get('register:one')).resolves.toEqual({ given_name: 'Neema' })
    await expect(store.get('register:two')).resolves.toEqual({ given_name: 'Joseph' })
  })

  // Cleared ONLY after a confirmed server write.
  test('clearing removes just that key', async () => {
    await store.set('register:one', { given_name: 'Neema' })
    await store.set('register:two', { given_name: 'Joseph' })
    await store.clear('register:one')
    await expect(store.get('register:one')).resolves.toBeUndefined()
    await expect(store.get('register:two')).resolves.toEqual({ given_name: 'Joseph' })
  })

  test('clearing a key that was never written is not an error', async () => {
    await expect(store.clear('nope')).resolves.toBeUndefined()
  })
})

describe('draftKey', () => {
  test('is form name plus client_ref, per business-rules §12', async () => {
    const { draftKey } = await import('@/lib/drafts')
    expect(draftKey('register', 'c0ffee')).toBe('register:c0ffee')
  })
})

describe('store failure is not fatal', () => {
  // A browser with storage disabled must not take the form down with it. The
  // draft is a convenience; losing it is worse than crashing is not.
  test('a failing get resolves to undefined rather than throwing', async () => {
    const broken: DraftStore = {
      get: vi.fn().mockRejectedValue(new Error('storage disabled')),
      set: vi.fn(),
      clear: vi.fn(),
    }
    const { safeGet } = await import('@/lib/drafts')
    await expect(safeGet(broken, 'register:abc')).resolves.toBeUndefined()
  })

  test('a failing set is swallowed, so typing continues to work', async () => {
    const broken: DraftStore = {
      get: vi.fn(),
      set: vi.fn().mockRejectedValue(new Error('quota exceeded')),
      clear: vi.fn(),
    }
    const { safeSet } = await import('@/lib/drafts')
    await expect(safeSet(broken, 'register:abc', { a: 1 })).resolves.toBe(false)
  })

  test('a successful set reports success', async () => {
    const { safeSet } = await import('@/lib/drafts')
    await expect(safeSet(store, 'register:abc', { a: 1 })).resolves.toBe(true)
  })
})
