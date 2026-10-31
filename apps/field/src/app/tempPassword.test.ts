import { beforeEach, expect, test } from 'vitest'

import { clearTempPassword, holdTempPassword, peekTempPassword } from '@/app/tempPassword'

beforeEach(() => {
  clearTempPassword()
  localStorage.clear()
  sessionStorage.clear()
})

test('holds the password in memory until cleared', () => {
  expect(peekTempPassword()).toBeNull()
  holdTempPassword('k7qxm2pa')
  expect(peekTempPassword()).toBe('k7qxm2pa')
  clearTempPassword()
  expect(peekTempPassword()).toBeNull()
})

test('never touches browser storage', () => {
  holdTempPassword('k7qxm2pa')
  expect(localStorage.length).toBe(0)
  expect(sessionStorage.length).toBe(0)
})
