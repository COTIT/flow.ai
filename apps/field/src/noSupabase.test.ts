import { readdirSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, test } from 'vitest'

import { code, offendingLines, read } from './styles/design'

/**
 * The app runs on the Genesy API only. Supabase was RUAHA's backend and was
 * removed in port step 8; this guard keeps it from creeping back (tests
 * included, so no stray `vi.mock('@/lib/supabase')` survives either).
 */

const SUPABASE = /['"](@supabase\/|@\/lib\/supabase\b|@\/lib\/db\.types\b)/

const files = readdirSync('src', { recursive: true, encoding: 'utf8' })
  .filter((file) => /\.(ts|tsx)$/.test(file))
  .map((file) => join('src', file))

describe('nothing in src reaches Supabase', () => {
  test('there are files to check', () => {
    expect(files.length).toBeGreaterThan(50)
  })

  test.each(files)('%s', (file) => {
    expect(offendingLines(code(file), SUPABASE)).toEqual([])
  })
})

test('package.json has no Supabase dependency', () => {
  const { dependencies = {}, devDependencies = {} } = JSON.parse(read('package.json')) as {
    dependencies?: Record<string, string>
    devDependencies?: Record<string, string>
  }
  expect(Object.keys({ ...dependencies, ...devDependencies }).filter((name) => name.includes('supabase'))).toEqual([])
})
