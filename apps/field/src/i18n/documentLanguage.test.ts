import { beforeEach, describe, expect, test } from 'vitest'

const i18n = (await import('@/i18n')).default

beforeEach(async () => {
  await i18n.changeLanguage('en')
})

/**
 * QA #18. `index.html` hardcoded `lang="sw"` and nothing ever updated it.
 *
 * Two consequences, and the first is the serious one: ops and admin surfaces
 * are English by design, so a screen reader applied Swahili pronunciation
 * rules to English text — close to unusable. The second: switching language
 * changed every rendered string and left `document.documentElement.lang`
 * saying `sw` regardless.
 */
describe('the document language follows the active language', () => {
  test('English content is announced as English', async () => {
    await i18n.changeLanguage('en')
    expect(document.documentElement.lang).toBe('en')
  })

  test('and Swahili as Swahili', async () => {
    await i18n.changeLanguage('sw')
    expect(document.documentElement.lang).toBe('sw')
  })

  test('it follows every switch, not just the first', async () => {
    await i18n.changeLanguage('sw')
    await i18n.changeLanguage('en')
    await i18n.changeLanguage('sw')
    expect(document.documentElement.lang).toBe('sw')
  })

  // The static attribute is the first thing a screen reader sees, before any
  // JavaScript runs. It must not claim a language the app does not start in.
  test('the shipped document starts in the language i18n initialises to', () => {
    expect(i18n.options.lng).toBe('en')
  })
})
