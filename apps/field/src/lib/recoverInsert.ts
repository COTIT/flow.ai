import { EarlierVersionSavedError } from '@/lib/drafts'
import { supabase } from '@/lib/supabase'

const blank = (value: unknown) => value === null || value === undefined || value === ''

/** PostgREST may return numeric as a string; a blank field is sent as null. */
function same(stored: unknown, submitted: unknown) {
  if (blank(stored) || blank(submitted)) return blank(stored) && blank(submitted)
  if (typeof stored === 'number' || typeof submitted === 'number') return Number(stored) === Number(submitted)
  return stored === submitted
}

/**
 * A timed-out response can follow a committed insert. Never blindly replay it:
 * the row under this id is the success only when it holds what was submitted.
 */
export async function recoverInsert(
  table: 'pue_request' | 'buyer' | 'buyer_demand' | 'opportunity_supply' | 'equipment' | 'loan_product',
  id: string | undefined,
  error: { message: string },
  submitted: Record<string, unknown>,
) {
  if (id) {
    const columns = Object.keys(submitted).filter(column => column !== 'id')
    const result = await supabase.from(table).select(['id', ...columns].join(',')).eq('id', id).maybeSingle()
    const row = result.data as Record<string, unknown> | null
    if (!result.error && row) {
      if (columns.every(column => same(row[column], submitted[column]))) return { id }
      throw new EarlierVersionSavedError()
    }
  }
  throw new Error(error.message)
}
