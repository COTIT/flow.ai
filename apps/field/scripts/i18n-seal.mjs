#!/usr/bin/env node
/**
 * Records, for every Swahili string, a fingerprint of the English it was
 * written from: `src/i18n/sw/en-source.json`.
 *
 *   pnpm i18n:seal
 *
 * Run it AFTER translating or re-translating. `bundles.test.ts` fails when an
 * English string has changed since its Swahili was sealed, which is how a
 * stale translation gets noticed. Sealing is a claim that the Swahili matches
 * the English as it stands today, so do not run it to make the test pass
 * without reading what changed.
 */
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { englishFingerprint, flatten } from './i18n-handover.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const read = (path) => JSON.parse(readFileSync(resolve(root, path), 'utf8'))

const en = flatten(read('src/i18n/en/common.json'))
const sw = flatten(read('src/i18n/sw/common.json'))
const sealed = {}
for (const key of Object.keys(en)) if (key in sw) sealed[key] = englishFingerprint(en[key])
writeFileSync(resolve(root, 'src/i18n/sw/en-source.json'), JSON.stringify(sealed, null, 2) + '\n')
console.log(`sealed ${Object.keys(sealed).length} strings`)
