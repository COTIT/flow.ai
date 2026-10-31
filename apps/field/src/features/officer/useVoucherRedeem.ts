import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  farmVouchersLookup,
  farmVouchersRedeem,
  farmVouchersTimeline,
} from '@genesy/api-client/endpoints/farm-vouchers'
import type { Money, Redeemed } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import type { AuditEvent } from '@/components/AuditTimeline'
import { queryKeys } from '@/lib/queryKeys'
import type { Database } from '@/lib/db.types'

export type IdDocumentType = Database['public']['Enums']['id_document_type']
type VoucherStatus = Database['public']['Enums']['voucher_status']

/** `POST /farm/vouchers/lookup` when the code names a voucher this caller may see. */
export interface VoucherPreview {
  found: true
  voucher_id: string
  status: VoucherStatus
  /** Derived by the API: issued, and past `expires_at`. */
  expired: boolean
  amount: Money
  issued_at: string
  expires_at: string
  redeemed_at: string | null
  survey_title_en: string | null
  survey_title_sw: string | null
  household_label: string
  head_name: string | null
  respondent_name: string | null
  needs_ops: boolean
  can_redeem: boolean
  blocked_reason: string | null
}

/**
 * An unknown code and a code outside the caller's villages give the same
 * answer on purpose, so a scan cannot be used to probe for vouchers.
 */
export type VoucherLookup = { found: false } | VoucherPreview

/** `replayed`: a retry by the same staff member after a dropped connection; no second payout. */
export type VoucherRedemption = Redeemed

export interface RedeemInput {
  code: string
  /** Null when none was chosen: the API refuses it in its own words. */
  idType: IdDocumentType | null
  nameConfirmed: boolean
}

export async function lookupVoucher(code: string): Promise<VoucherLookup> {
  // The generated type marks every field optional; with found:true the API fills them.
  return (await farmVouchersLookup(workspaceId(), { code })).data as VoucherLookup
}

/**
 * Hand over the incentive. Online only, never queued. Every rule — four eyes on
 * the household, the audit hold, expiry, a second redeem, the ID and the name
 * check — is the API's, and its refusal is the message the officer reads.
 */
export async function redeemVoucher({ code, idType, nameConfirmed }: RedeemInput): Promise<VoucherRedemption> {
  // A blank id_type reaches the API as "none", which it refuses with
  // 'record which ID document you checked': the sentence the officer should see.
  const body = { code, id_type: idType ?? '', name_confirmed: nameConfirmed }
  return (await farmVouchersRedeem(workspaceId(), body)).data
}

/** The voucher's audit trail, already filtered by the caller's role. */
export async function fetchVoucherTimeline(voucherId: string): Promise<AuditEvent[]> {
  return (await farmVouchersTimeline(workspaceId(), voucherId)).data
}

/**
 * A MUTATION, not a query: every call writes a 'scanned' audit event. A query
 * would refetch on focus, remount or invalidation and put scans in the trail
 * that nobody performed. `gcTime: 0` so the code in its variables does not
 * outlive the screen.
 */
export function useVoucherLookup() {
  return useMutation({ mutationFn: lookupVoucher, gcTime: 0 })
}

export function useVoucherRedeem() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: redeemVoucher,
    gcTime: 0,
    onSuccess: async (result) => {
      // The voucher's own trail, then the ops views that count redemptions —
      // by prefix, because this screen does not know their survey or range.
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.voucherTimeline(result.voucher_id) }),
        queryClient.invalidateQueries({ queryKey: ['surveyVouchers'] }),
        queryClient.invalidateQueries({ queryKey: ['surveyAdmin'] }),
        queryClient.invalidateQueries({ queryKey: ['redemptionLog'] }),
      ])
    },
  })
}

/** Read only once there is a voucher to read: after a redeem. */
export function useVoucherTimeline(voucherId: string | null) {
  return useQuery({
    queryKey: queryKeys.voucherTimeline(voucherId ?? ''),
    queryFn: () => fetchVoucherTimeline(voucherId as string),
    enabled: voucherId !== null,
  })
}
