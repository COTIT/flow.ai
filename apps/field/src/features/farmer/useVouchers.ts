import { useCallback } from 'react'
import { useQuery } from '@tanstack/react-query'
import { farmVouchersMine } from '@genesy/api-client/endpoints/farm-vouchers'
import type { MyVoucher } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import type { AuditEvent } from '@/components/AuditTimeline'
import type { VoucherDisplayStatus } from '@/components/StatusPill'
import { allPages } from '@/features/farmer/pages'
import { queryKeys } from '@/lib/queryKeys'

/**
 * One of the household's incentive vouchers, with its code. `GET
 * /farm/vouchers/mine` is the only place a code is ever shown; the audit hold
 * is never sent.
 */
export type FarmerVoucher = MyVoucher

/** The household's own vouchers. The API scopes them to the caller; nothing is filtered here. */
export function fetchFarmerVouchers(): Promise<FarmerVoucher[]> {
  return allPages((params) => farmVouchersMine(workspaceId(), params))
}

export function useFarmerVouchers() {
  return useQuery({ queryKey: queryKeys.farmerVouchers(), queryFn: fetchFarmerVouchers })
}

/**
 * One voucher, picked out of the household list — the same cache entry, so
 * the survey list and the voucher card cannot show two different statuses.
 * `null` when the list does not hold it: an answer, not an error.
 */
export function useVoucher(voucherId: string) {
  const pick = useCallback(
    (rows: FarmerVoucher[]) => rows.find((row) => row.id === voucherId) ?? null,
    [voucherId],
  )
  return useQuery({
    queryKey: queryKeys.farmerVouchers(),
    queryFn: fetchFarmerVouchers,
    select: pick,
  })
}

/** Who handed the incentive over: the latest `redeemed` event's snapshot name. */
export function redeemedByName(events: AuditEvent[]): string | null {
  const redeemed = events.filter((event) => event.kind === 'redeemed')
  return redeemed.at(-1)?.actor_name ?? null
}

/**
 * `expired` is the API's (issued, and past `expires_at`). Only an uncollected
 * voucher expires — a collected or cancelled one keeps the status that says
 * what happened to it.
 */
export function voucherDisplayStatus(voucher: Pick<FarmerVoucher, 'status' | 'expired'>): VoucherDisplayStatus {
  if (voucher.status === 'issued' && voucher.expired) return 'expired'
  return voucher.status as VoucherDisplayStatus
}
