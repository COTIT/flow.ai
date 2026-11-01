import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { farmLoansList } from '@genesy/api-client/endpoints/farm-loans'
import type { Money } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import { queryKeys } from '@/lib/queryKeys'

export interface LoanItem {
  id: string
  code: string
  name: string
  description: string | null
  indicative_min: Money
  indicative_max: Money
}

/**
 * Loan LISTINGS in the resource catalogue — read by the farmer's Resources screen and the ops
 * catalogue alike; every farm member of the account reads them.
 *
 * A listing only: a name, a description and an INDICATIVE range in integer minor units. The API
 * holds no interest, deposit, term or repayment, because research item C is unresolved.
 */
export function useLoanProducts() {
  const { i18n } = useTranslation()
  const query = useQuery({
    queryKey: queryKeys.loanProducts(),
    queryFn: () => {
      const workspace = workspaceId()
      return allPages((params) => farmLoansList(workspace, { active: true, ...params }))
    },
    staleTime: 5 * 60_000,
  })
  const sw = i18n.resolvedLanguage === 'sw'
  const items: LoanItem[] = (query.data ?? []).map((row) => ({
    id: row.id,
    code: row.code,
    name: sw ? row.name_sw : row.name_en,
    description: sw ? row.description_sw : row.description_en,
    indicative_min: row.indicative_min,
    indicative_max: row.indicative_max,
  }))
  return { ...query, items }
}
