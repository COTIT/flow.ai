import { isRedirect } from '@tanstack/react-router'
import { expect, test } from 'vitest'

import { guardResources } from '@/lib/features'

test('with the flag off, a resource route redirects to the surface home', () => {
  let thrown: unknown
  try {
    guardResources('/farm', false)()
  } catch (error) {
    thrown = error
  }
  expect(isRedirect(thrown)).toBe(true)
  expect((thrown as { options: { to: string } }).options.to).toBe('/farm')
})

test('with the flag on, the route loads', () => {
  expect(() => guardResources('/ops', true)()).not.toThrow()
})
