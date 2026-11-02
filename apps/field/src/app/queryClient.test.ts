import { expect, test } from 'vitest'

import { queryClient } from '@/app/queryClient'

/**
 * TanStack's default `networkMode: 'online'` PAUSES queries and saves while the
 * browser says offline: the screen sits on "Loading…" (or falls through to an
 * empty list) with no message. The field app sends anyway, so the request fails
 * fast into "No network" with a retry.
 */
test('queries and saves are sent even while the browser says offline', () => {
  const defaults = queryClient.getDefaultOptions()
  expect(defaults.queries?.networkMode).toBe('always')
  expect(defaults.mutations?.networkMode).toBe('always')
})
