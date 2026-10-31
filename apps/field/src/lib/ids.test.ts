import { join } from 'node:path'

import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

import { isUuid, newUuid } from '@/lib/ids'
import { code, offendingLines, sourceFiles } from '@/styles/design'

/**
 * QA #3. `/officer/people/not-a-uuid` rendered
 * `invalid input syntax for type uuid: "not-a-uuid"` — database internals as
 * user copy, on every `$id` route.
 *
 * A well-formed id matching nothing already behaves correctly: RLS returns
 * zero rows and the screen says "not found". That is the behaviour a malformed
 * id should reach too, which means recognising one BEFORE the query rather
 * than translating the failure afterwards.
 */
describe('isUuid', () => {
  test('accepts the ids the seed uses', () => {
    expect(isUuid('30000000-0000-4000-8000-000000000001')).toBe(true)
    expect(isUuid('e2000000-0000-4000-8000-000000000001')).toBe(true)
  })

  test('accepts a generated v4', () => {
    expect(isUuid(crypto.randomUUID())).toBe(true)
  })

  test('is case-insensitive, as Postgres is', () => {
    expect(isUuid('E2000000-0000-4000-8000-000000000001')).toBe(true)
  })

  test.each([
    'not-a-uuid',
    '',
    '   ',
    '30000000-0000-4000-8000',
    '30000000-0000-4000-8000-0000000000011',
    '30000000_0000_4000_8000_000000000001',
    'zzzzzzzz-0000-4000-8000-000000000001',
    // The shape Postgres accepts without dashes is deliberately NOT accepted:
    // nothing in this app produces one, so it is far likelier to be junk.
    '30000000000040008000000000000001',
  ])('rejects %s', (value) => {
    expect(isUuid(value)).toBe(false)
  })

  test('rejects a missing value without throwing', () => {
    expect(isUuid(undefined)).toBe(false)
    expect(isUuid(null)).toBe(false)
  })

  // An id with surrounding space came from a URL nobody typed carefully; it is
  // not this function's job to repair it.
  test('does not trim', () => {
    expect(isUuid(' 30000000-0000-4000-8000-000000000001 ')).toBe(false)
  })
})

/**
 * `crypto.randomUUID` exists only in a SECURE CONTEXT. `localhost` is one, so
 * `pnpm dev`, Vitest and Playwright never saw this; `http://192.168.100.42:5173`
 * — the LAN address a stakeholder opens on a phone, and the way this demo is
 * actually shown — is not, and the register screen died on
 * `crypto.randomUUID is not a function` before it rendered a field.
 *
 * `crypto.getRandomValues` carries no such restriction, so the fallback is a
 * real v4 rather than a weaker id: same entropy, same shape, no `Math.random`.
 */
describe('newUuid', () => {
  test('returns something isUuid accepts', () => {
    expect(isUuid(newUuid())).toBe(true)
  })

  test('does not repeat', () => {
    expect(new Set(Array.from({ length: 64 }, newUuid)).size).toBe(64)
  })

  test('is a v4 with the right variant bits', () => {
    const uuid = newUuid()
    expect(uuid[14]).toBe('4')
    expect('89ab').toContain(uuid[19])
  })

  // `randomUUID` lives on Crypto.prototype, so it is shadowed with an own
  // property and the shadow removed again — deleting it outright does nothing.
  describe('without crypto.randomUUID — an insecure context', () => {
    beforeEach(() => {
      Object.defineProperty(crypto, 'randomUUID', { value: undefined, configurable: true })
    })

    afterEach(() => {
      Reflect.deleteProperty(crypto, 'randomUUID')
    })

    test('still produces a v4, rather than throwing', () => {
      expect(crypto.randomUUID).toBeUndefined()
      const uuid = newUuid()
      expect(isUuid(uuid)).toBe(true)
      expect(uuid[14]).toBe('4')
    })

    test('draws from getRandomValues, not Math.random', () => {
      const spy = vi.spyOn(crypto, 'getRandomValues')
      const random = vi.spyOn(Math, 'random')
      newUuid()
      expect(spy).toHaveBeenCalled()
      expect(random).not.toHaveBeenCalled()
      spy.mockRestore()
      random.mockRestore()
    })
  })
})

/**
 * The guard, in the idiom of the design guards: what failed was not one line,
 * it is a call anybody would write again. Every id in the app comes from here.
 */
describe('crypto.randomUUID is never called directly', () => {
  // Everywhere but here, where newUuid() prefers it when a secure context has it.
  const files = sourceFiles('src', ['.ts', '.tsx']).filter((file) => file !== join('src', 'lib', 'ids.ts'))

  test.each(files)('%s', (file) => {
    expect(
      offendingLines(code(file), /crypto\s*\.\s*randomUUID/),
      'use newUuid() from @/lib/ids — crypto.randomUUID is undefined outside a secure context',
    ).toEqual([])
  })
})
