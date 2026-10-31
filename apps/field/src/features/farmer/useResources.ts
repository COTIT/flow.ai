import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { Database } from '@/lib/db.types'

type LoanRow = Database['public']['Tables']['loan_product']['Row']

export interface LoanItem {
  id: string
  code: string
  name: string
  description: string | null
  indicative_min_amount: number
  indicative_max_amount: number
  currency: string
}

/**
 * Loan LISTINGS in the resource catalogue — read by the farmer's Resources
 * screen and the ops catalogue alike, scoped by `loan_product_read` to
 * `app_projects()`.
 *
 * A listing only: a name, a description and an INDICATIVE range. The table
 * holds no interest, deposit, term or repayment, because research item C is
 * unresolved. Names and descriptions are translated in the database.
 */
export async function fetchLoanProducts(): Promise<LoanRow[]> {
  const { data, error } = await supabase
    .from('loan_product')
    .select('*')
    .eq('is_active', true)
    .order('name_en')

  if (error) throw new Error(error.message)
  return data ?? []
}

export function useLoanProducts() {
  const { i18n } = useTranslation()
  const query = useQuery({ queryKey: queryKeys.loanProducts(), queryFn: fetchLoanProducts, staleTime: 5 * 60_000 })
  const sw = i18n.resolvedLanguage === 'sw'
  const items: LoanItem[] = (query.data ?? []).map((row) => ({
    id: row.id,
    code: row.code,
    name: sw ? row.name_sw : row.name_en,
    description: sw ? row.description_sw : row.description_en,
    indicative_min_amount: row.indicative_min_amount,
    indicative_max_amount: row.indicative_max_amount,
    currency: row.currency,
  }))
  return { ...query, items }
}
