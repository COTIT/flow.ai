import { describe, expect, test } from 'vitest'

import {
  SURFACE_OF,
  buildHandover,
  englishFingerprint,
  flatten,
  renderCsv,
  renderHandover,
  reviewedKeysOf,
  validateReviewed,
  validateSwahili,
  wordingViolations,
} from '../../scripts/i18n-handover.mjs'

const en = {
  common: { loading: 'Loading…' },
  register: { title: 'Register a farmer', harvestKg: 'Expected harvest (kg)' },
  tower: { drill: 'See the records' },
  estimate: { isEstimate: 'This is an estimate, not a measurement.' },
  language: { en: 'English', sw: 'Kiswahili' },
}
const sw = { language: { en: 'Kiingereza', sw: 'Kiswahili' } }

describe('flatten', () => {
  test('turns nested namespaces into dotted keys', () => {
    expect(flatten({ a: { b: 'x' }, c: 'y' })).toEqual({ 'a.b': 'x', c: 'y' })
  })

  test('leaves a flat object alone', () => {
    expect(flatten({ a: 'x' })).toEqual({ a: 'x' })
  })
})

/**
 * CLAUDE.md: "Farmer and Officer surfaces ship complete Swahili. Ops and Tower
 * may ship English for the demo." A reviewer's time is the scarce resource, so
 * the handover has to say which strings actually block the demo.
 */
describe('which surface a key belongs to', () => {
  test('farmer and officer namespaces are required', () => {
    expect(SURFACE_OF('register.title')).toBe('required')
    expect(SURFACE_OF('equipment.submit')).toBe('required')
    expect(SURFACE_OF('farmHome.title')).toBe('required')
  })

  test('shared chrome is required too — both surfaces render it', () => {
    expect(SURFACE_OF('common.loading')).toBe('required')
    expect(SURFACE_OF('error.network')).toBe('required')
    expect(SURFACE_OF('nav.signOut')).toBe('required')
  })

  test('ops and tower may ship English', () => {
    expect(SURFACE_OF('tower.drill')).toBe('optional')
    expect(SURFACE_OF('demand.colBuyer')).toBe('optional')
    expect(SURFACE_OF('buyers.title')).toBe('optional')
  })

  // Admin survey authoring and the ops tour only ever render inside the ops
  // shell. Counting them as required put 119 strings on the reviewer's list
  // that block nothing.
  test('admin survey authoring and the ops tour are ops-only', () => {
    expect(SURFACE_OF('surveyAdmin.title')).toBe('optional')
    expect(SURFACE_OF('tour.ops.step1')).toBe('optional')
  })

  test('the farmer and officer tours, and the tour chrome, stay required', () => {
    expect(SURFACE_OF('tour.farmer.step1')).toBe('required')
    expect(SURFACE_OF('tour.officer.step1')).toBe('required')
    expect(SURFACE_OF('tour.next')).toBe('required')
  })

  test('opportunity status appears on farmer surface', () => {
    expect(SURFACE_OF('opportunityStatus.proposed')).toBe('required')
  })

  // An unrecognised namespace is REQUIRED, not optional: a new farmer-facing
  // namespace must not slip out of the reviewer's list by being forgotten here.
  test('an unknown namespace is assumed to block the demo', () => {
    expect(SURFACE_OF('somethingNew.title')).toBe('required')
  })
})

describe('Swahili review checks', () => {
  test('rejects changed placeholders and half-translated plurals', () => {
    expect(validateSwahili(
      { count_one: '{{count}} item', count_other: '{{count}} items' },
      { count_one: 'Kitu {{value}}' },
    )).toEqual([
      'count_one: interpolation placeholders differ',
      'count_one: plural _other missing',
    ])
  })
})

describe('buildHandover', () => {
  test('carries every English key', () => {
    const rows = buildHandover(en, sw)
    expect(rows).toHaveLength(Object.keys(flatten(en)).length)
  })

  test('shows the English string a reviewer has to translate', () => {
    const row = buildHandover(en, sw).find((r) => r.key === 'register.title')
    expect(row?.english).toBe('Register a farmer')
  })

  // The two attested keys are already done, and asking for them again wastes
  // the reviewer's attention.
  test('marks what already has Swahili, with the existing wording', () => {
    const row = buildHandover(en, sw).find((r) => r.key === 'language.sw')
    expect(row?.swahili).toBe('Kiswahili')
    expect(row?.translated).toBe(true)
  })

  test('and leaves the rest empty rather than guessing', () => {
    const row = buildHandover(en, sw).find((r) => r.key === 'register.title')
    expect(row?.swahili).toBe('')
    expect(row?.translated).toBe(false)
  })

  // A draft is not a reviewed string. The packet has to say which is which, or
  // the reviewer cannot tell what has already been through a native reader.
  // A drafter's or cross-check's doubt has to reach the reviewer, or it dies in
  // a scratch file.
  describe('flags', () => {
    test('a flagged string carries the reason in its row', () => {
      const rows = buildHandover(en, sw, new Set(), { 'register.title': 'Drafter: no Tanzanian source.' })
      expect(rows.find((r) => r.key === 'register.title')?.flag).toBe('Drafter: no Tanzanian source.')
    })

    test('an unflagged string has an empty flag', () => {
      expect(buildHandover(en, sw).find((r) => r.key === 'register.title')?.flag).toBe('')
    })

    test('the flag appears in the notes column of the packet and the CSV', () => {
      const rows = buildHandover(en, sw, new Set(), { 'register.title': 'Check this one.' })
      const doc = renderHandover(rows, { generated: '2026-09-13' })
      expect(doc).toMatch(/Flag: Check this one\./)
      expect(renderCsv(rows)).toMatch(/Flag: Check this one\./)
    })

    test('a flagged string is counted in the packet header', () => {
      const rows = buildHandover(en, sw, new Set(), { 'register.title': 'x' })
      expect(renderHandover(rows, { generated: '2026-09-13' })).toMatch(/1 flagged/)
    })
  })

  describe('review status', () => {
    const drafted = { ...sw, register: { title: 'Sajili mkulima' } }

    test('a string with no Swahili is missing', () => {
      const row = buildHandover(en, sw).find((r) => r.key === 'register.harvestKg')
      expect(row?.status).toBe('missing')
    })

    test('a string with Swahili nobody has reviewed is a draft', () => {
      const row = buildHandover(en, drafted).find((r) => r.key === 'register.title')
      expect(row?.status).toBe('draft')
    })

    test('a string on the reviewed list is reviewed', () => {
      const row = buildHandover(en, drafted, new Set(['register.title'])).find(
        (r) => r.key === 'register.title',
      )
      expect(row?.status).toBe('reviewed')
    })

    test('reviewed is a claim, so it needs the string to exist', () => {
      const row = buildHandover(en, sw, new Set(['register.title'])).find(
        (r) => r.key === 'register.title',
      )
      expect(row?.status).toBe('missing')
    })
  })

  /**
   * Language and labelling in CLAUDE.md are product requirements, not copy
   * preferences: "Getting them wrong misrepresents the programme." A reviewer
   * who does not know that will translate "estimate" into something confident.
   */
  test('attaches the labelling rule to the strings it governs', () => {
    const row = buildHandover(en, sw).find((r) => r.key === 'estimate.isEstimate')
    expect(row?.note).toMatch(/estimate/i)
  })

  test('a string with no rule attached carries no invented note', () => {
    const row = buildHandover(en, sw).find((r) => r.key === 'common.loading')
    expect(row?.note).toBe('')
  })

  test('required rows come first, so the blocking work is at the top', () => {
    const rows = buildHandover(en, sw)
    const firstOptional = rows.findIndex((r) => r.surface === 'optional')
    const lastRequired = rows.map((r) => r.surface).lastIndexOf('required')
    expect(lastRequired).toBeLessThan(firstOptional)
  })

  test('interpolation placeholders are flagged, because they must survive', () => {
    const rows = buildHandover({ a: { b: 'Stored as {{value}}' } }, {})
    expect(rows[0].placeholders).toEqual(['{{value}}'])
  })

  test('a string with no placeholders reports none', () => {
    expect(buildHandover({ a: { b: 'Plain' } }, {})[0].placeholders).toEqual([])
  })
})

describe('renderHandover', () => {
  const doc = () => renderHandover(buildHandover(en, sw), { generated: '2026-09-13' })

  test('states the rule it exists to serve', () => {
    expect(doc()).toMatch(/farmer and officer/i)
    expect(doc()).toMatch(/ops and tower/i)
  })

  // The instruction that matters most to whoever picks this up next: the
  // Swahili here is a DRAFT, and the reviewer's corrections are the point.
  test('says plainly that the Swahili is an unreviewed draft', () => {
    expect(doc()).toMatch(/draft/i)
    expect(doc()).toMatch(/native reviewer/i)
    expect(doc()).not.toMatch(/nothing here may be machine translated/i)
  })

  // The fixture holds seven keys: six required, one on the tower surface, and
  // two that already carry Swahili.
  test('counts the work rather than making the reviewer count it', () => {
    const rendered = doc()
    expect(rendered).toMatch(/6 strings/)
    expect(rendered).toMatch(/1 strings/)
    expect(rendered).toMatch(/7 strings in total: 0 reviewed, 2 draft, 5 missing/)
  })

  test('carries a status column so a reviewer can see what is still a draft', () => {
    expect(doc()).toMatch(/\| Key \| English \| Swahili \| Status \| Notes \|/)
  })

  test('every key appears with its English string', () => {
    expect(doc()).toContain('register.title')
    expect(doc()).toContain('Register a farmer')
  })

  test('a pipe in a string does not break the table it sits in', () => {
    const rendered = renderHandover(buildHandover({ a: { b: 'one | two' } }, {}), {
      generated: '2026-09-13',
    })
    const row = rendered.split('\n').find((line) => line.startsWith('| `a.b`'))
    // Five columns plus the leading and trailing delimiters.
    expect(row?.split('|').length).toBe(8)
  })
})

describe('renderCsv', () => {
  const rows = buildHandover(en, { ...sw, register: { title: 'Sajili mkulima' } })
  const csv = () => renderCsv(rows)

  test('starts with a header a spreadsheet can read', () => {
    expect(csv().split('\n')[0]).toBe('key,surface,english,swahili,status,notes')
  })

  test('has one line per key', () => {
    expect(csv().split('\n')).toHaveLength(rows.length + 1)
  })

  // A comma or a quote in a string would otherwise shift every column after it.
  test('quotes a field that holds a comma, a quote or a newline', () => {
    const tricky = renderCsv(
      buildHandover({ a: { b: 'Say "hi", then\nleave' } }, {}),
    )
    expect(tricky).toContain('"Say ""hi"", then\nleave"')
  })

  test('carries the status of each string', () => {
    expect(csv()).toMatch(/register\.title,required,Register a farmer,Sajili mkulima,draft,/)
  })
})

describe('the reviewed list', () => {
  const swahili = { register: { title: 'Sajili mkulima' }, common: { loading: 'Inapakia…' } }
  const good = { reviewer: 'A. Reviewer, UDSM', date: '2026-10-02', keys: ['register.title'] }

  test('a batch that names a reviewer, a date and real keys is fine', () => {
    expect(validateReviewed([good], swahili)).toEqual([])
  })

  // Reviewed is a claim that a native reader supplied or approved the string.
  // A claim with nobody's name on it is not one.
  test('rejects a batch with no reviewer', () => {
    expect(validateReviewed([{ ...good, reviewer: '  ' }], swahili)).toEqual([
      'batch 1: reviewer is blank',
    ])
  })

  test('rejects a batch with no real date', () => {
    expect(validateReviewed([{ ...good, date: 'soon' }], swahili)).toEqual([
      'batch 1: date must be YYYY-MM-DD',
    ])
  })

  test('rejects a reviewed key that has no Swahili to review', () => {
    expect(validateReviewed([{ ...good, keys: ['register.title', 'nope.x'] }], swahili)).toEqual([
      'batch 1: nope.x has no Swahili',
    ])
  })

  test('rejects a key claimed by two batches', () => {
    const second = { ...good, reviewer: 'B. Reviewer', date: '2026-10-03' }
    expect(validateReviewed([good, second], swahili)).toEqual([
      'batch 2: register.title is already reviewed in batch 1',
    ])
  })

  test('flattens the batches into one set of reviewed keys', () => {
    const second = { reviewer: 'B', date: '2026-10-03', keys: ['common.loading'] }
    expect([...reviewedKeysOf([good, second])].sort()).toEqual(['common.loading', 'register.title'])
  })
})

/**
 * CLAUDE.md: a survey incentive is "a fixed cash amount per household per
 * survey, paid at the office. Never earnings, wallet, balance or payment."
 * Nothing tested that in English, and a translator can break it in Swahili
 * without anyone noticing. The forbidden list is derived, not sourced — it
 * exists to make a reviewer look, not to certify the wording.
 */
describe('incentive wording', () => {
  const FORBIDDEN = /\b(mapato|pochi|salio|malipo|mshahara)\b/i

  test('passes an incentive string that says a fixed cash amount', () => {
    expect(
      wordingViolations({ voucher: { amount: 'Kiasi maalum cha pesa taslimu' } }, ['voucher'], FORBIDDEN),
    ).toEqual([])
  })

  test('flags an incentive string that reads as earnings or a balance', () => {
    expect(
      wordingViolations(
        { voucher: { amount: 'Mapato yako' }, surveys: { hint: 'Salio lako' } },
        ['voucher', 'surveys'],
        FORBIDDEN,
      ),
    ).toEqual(['voucher.amount: "Mapato yako"', 'surveys.hint: "Salio lako"'])
  })

  test('leaves alone a namespace that is not about the incentive', () => {
    expect(wordingViolations({ opportunity: { notASale: 'Si malipo' } }, ['voucher'], FORBIDDEN)).toEqual([])
  })
})

describe('englishFingerprint', () => {
  test('is stable for the same English', () => {
    expect(englishFingerprint('Seven short stops.')).toBe(englishFingerprint('Seven short stops.'))
  })

  test('changes when the English changes, even by a character', () => {
    expect(englishFingerprint('Seven short stops.')).not.toBe(englishFingerprint('Seven short stops'))
  })

  test('is short enough to keep a thousand of them readable', () => {
    expect(englishFingerprint('anything')).toMatch(/^[0-9a-f]{10}$/)
  })
})
