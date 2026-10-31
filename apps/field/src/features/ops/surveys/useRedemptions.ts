import { useQuery } from '@tanstack/react-query'
import {
  farmVouchersRedemptions,
  farmVouchersRedemptionTotals,
} from '@genesy/api-client/endpoints/farm-vouchers'
import type { Redemption, RedemptionTotal } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { queryKeys } from '@/lib/queryKeys'

export type RedemptionLogRow = Redemption
export type RedemptionTotalRow = RedemptionTotal

export interface RedemptionReport {
  /** Per officer per day, summed by the API. Never added up here. */
  totals: RedemptionTotalRow[]
  log: RedemptionLogRow[]
}

/** The API's largest page; the log is read to its last page. */
const PAGE_SIZE = 100

/** Both reads for one Tanzanian date range, cached together. */
export async function fetchRedemptions(from: string, to: string): Promise<RedemptionReport> {
  const workspace = workspaceId()
  const readLog = async () => {
    const log: RedemptionLogRow[] = []
    for (let page = 1, last = 1; page <= last; page++) {
      const reply = await farmVouchersRedemptions(workspace, { from, to, page, page_size: PAGE_SIZE })
      log.push(...reply.data)
      last = reply.meta.last_page
    }
    return log
  }
  const [totals, log] = await Promise.all([farmVouchersRedemptionTotals(workspace, { from, to }), readLog()])
  return { totals: totals.data, log }
}

export function useRedemptions(from: string, to: string) {
  return useQuery({
    queryKey: queryKeys.redemptionLog(from, to),
    queryFn: () => fetchRedemptions(from, to),
  })
}
