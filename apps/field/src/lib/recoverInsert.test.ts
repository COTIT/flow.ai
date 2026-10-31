import { beforeEach, describe, expect, test, vi } from 'vitest'

const maybeSingle = vi.fn()
const eq = vi.fn(() => ({ maybeSingle }))
const select = vi.fn((_columns: string) => ({ eq }))
const from = vi.fn((_table: string) => ({ select }))
vi.mock('@/lib/supabase', () => ({ supabase: { from: (table: string) => from(table) } }))

const { recoverInsert } = await import('@/lib/recoverInsert')
const { EarlierVersionSavedError, finishDraftWhenSaved } = await import('@/lib/drafts')
await import('@/i18n')

const ID = '11111111-1111-4111-8111-111111111111'
const BUYER = { project_id: 'p1', name: 'Mbeya Millers', channel: 'miller', contact_note: null }

beforeEach(() => {
  maybeSingle.mockReset()
  from.mockClear()
  select.mockClear()
  eq.mockClear()
})

describe('recoverInsert', () => {
  // A timeout can arrive after the row committed. The retry reuses the
  // clientRef, so a duplicate-key answer means the first attempt landed —
  // but only if it landed with the values being submitted now.
  test('a row under this id holding the submitted values is the success', async () => {
    maybeSingle.mockResolvedValue({ data: { id: ID, ...BUYER }, error: null })
    await expect(recoverInsert('buyer', ID, { message: 'Failed to fetch' }, BUYER)).resolves.toEqual({ id: ID })
    expect(from).toHaveBeenCalledWith('buyer')
    expect(select).toHaveBeenCalledWith('id,project_id,name,channel,contact_note')
    expect(eq).toHaveBeenCalledWith('id', ID)
  })

  // The draft keeps its clientRef after an uncertain failure. Edit a field and
  // resubmit, and the id collides with the EARLIER values: never "saved".
  test('a row under this id holding different values is refused', async () => {
    maybeSingle.mockResolvedValue({ data: { id: ID, ...BUYER, name: 'Old name' }, error: null })
    const attempt = recoverInsert('buyer', ID, { message: 'duplicate key' }, BUYER)
    await expect(attempt).rejects.toBeInstanceOf(EarlierVersionSavedError)
    await expect(attempt).rejects.toThrow('An earlier version of this form was already saved. Open it from the list to check it.')
  })

  // PostgREST can return numeric as a string; a blank form field is sent as
  // null. Neither is a difference.
  test('numeric strings and blanks compare by value', async () => {
    maybeSingle.mockResolvedValue({
      data: { id: ID, quantity_kg: '1200.00', indicative_price_per_kg: null, delivery_point: null },
      error: null,
    })
    await expect(recoverInsert('buyer_demand', ID, { message: 'x' }, {
      quantity_kg: 1200,
      indicative_price_per_kg: null,
      delivery_point: '',
    })).resolves.toEqual({ id: ID })
  })

  test('a blank against a value is a difference', async () => {
    maybeSingle.mockResolvedValue({ data: { id: ID, contributed_kg: 0 }, error: null })
    await expect(recoverInsert('opportunity_supply', ID, { message: 'x' }, { contributed_kg: null }))
      .rejects.toBeInstanceOf(EarlierVersionSavedError)
  })

  test('no row means the original error stands, verbatim', async () => {
    maybeSingle.mockResolvedValue({ data: null, error: null })
    const attempt = recoverInsert('pue_request', ID, { message: 'illegal transition' }, { quantity: 1 })
    await expect(attempt).rejects.toThrow('illegal transition')
    await expect(attempt).rejects.not.toBeInstanceOf(EarlierVersionSavedError)
  })

  test('a failed lookup never turns into a success', async () => {
    maybeSingle.mockResolvedValue({ data: null, error: { message: 'offline' } })
    await expect(recoverInsert('buyer_demand', ID, { message: 'Failed to fetch' }, { quantity_kg: 1 })).rejects.toThrow('Failed to fetch')
  })

  test('without an id nothing is looked up', async () => {
    await expect(recoverInsert('opportunity_supply', undefined, { message: 'x' }, {})).rejects.toThrow('x')
    expect(from).not.toHaveBeenCalled()
  })
})

describe('finishDraftWhenSaved', () => {
  test('a confirmed save clears the draft', async () => {
    const finish = vi.fn(async () => {})
    await finishDraftWhenSaved(Promise.resolve({ id: ID }), finish)
    expect(finish).toHaveBeenCalledTimes(1)
  })

  // The record exists, so the draft has nothing left to protect. The error
  // itself still shows through the mutation's own error state.
  test('an earlier saved version also clears it', async () => {
    const finish = vi.fn(async () => {})
    await finishDraftWhenSaved(Promise.reject(new EarlierVersionSavedError()), finish)
    expect(finish).toHaveBeenCalledTimes(1)
  })

  test('any other failure keeps the draft, and is not rethrown', async () => {
    const finish = vi.fn(async () => {})
    await expect(finishDraftWhenSaved(Promise.reject(new Error('over-commitment')), finish)).resolves.toBeUndefined()
    expect(finish).not.toHaveBeenCalled()
  })
})
