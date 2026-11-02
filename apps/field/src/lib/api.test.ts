import { expect, test } from 'vitest'

import { api } from '@/lib/api'

// A weak signal must end in "No network" with a retry, never an endless "Loading…".
test('a request with no answer gives up after 20 s', () => {
  expect(api.defaults.timeout).toBe(20_000)
})
