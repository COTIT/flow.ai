import { useCallback } from 'react'
import { useQuery } from '@tanstack/react-query'

import type { AuditEvent } from '@/components/AuditTimeline'
import type { VoucherDisplayStatus } from '@/components/StatusPill'
import type { Database } from '@/lib/db.types'
import { isUuid } from '@/lib/ids'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'

type VoucherStatus = Database['public']['Enums']['voucher_status']
type IdDocumentType = Database['public']['Enums']['id_document_type']

/**
 * Exactly the columns `survey_voucher` grants a client. `code` and
 * `audit_required` are left out of the grant, so naming either — or asking
 * for `*` — is refused with "permission denied". The household reads its code
 * through `app_voucher_code`; the audit hold is revealed only at the office.
 */
const VOUCHER_COLUMNS = `id, response_id, survey_id, household_id, village_id, amount, currency,
  status, issued_at, expires_at, redeemed_by, redeemed_at, id_type_seen,
  voided_by, voided_at, void_reason`

/** One of the household's incentive vouchers, without its code. */
export interface FarmerVoucher {
  id: string
  response_id: string
  survey_id: string
  household_id: string
  village_id: string
  amount: number
  currency: string
  status: VoucherStatus
  issued_at: string
  expires_at: string
  redeemed_by: string | null
  redeemed_at: string | null
  id_type_seen: IdDocumentType | null
  voided_by: string | null
  voided_at: string | null
  void_reason: string | null
}

/**
 * The household's own vouchers, newest first.
 *
 * No household filter: `survey_voucher_read` scopes the rows to
 * `app_households()`. Filtering here would imply the client is what keeps
 * another household's vouchers out.
 */
export async function fetchFarmerVouchers(): Promise<FarmerVoucher[]> {
  const { data, error } = await supabase
    .from('survey_voucher')
    .select(VOUCHER_COLUMNS)
    .order('issued_at', { ascending: false })

  if (error) throw new Error(error.message)

  return ((data ?? []) as unknown as FarmerVoucher[]).map((row) => ({
    ...row,
    // numeric(14,2): PostgREST may send it as text.
    amount: Number(row.amount),
  }))
}

export function useFarmerVouchers() {
  return useQuery({ queryKey: queryKeys.farmerVouchers(), queryFn: fetchFarmerVouchers })
}

/**
 * One voucher, picked out of the household list — the same cache entry, so
 * the survey list and the voucher card cannot show two different statuses.
 * `null` when RLS does not show it: an answer, not an error.
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

/**
 * The code, readable only by the household that owns the voucher. NULL for
 * anyone else, staff included — which is why staff scan it rather than look
 * it up.
 */
export async function fetchVoucherCode(voucherId: string): Promise<string | null> {
  if (!isUuid(voucherId)) return null
  const { data, error } = await supabase.rpc('app_voucher_code', { p_voucher_id: voucherId })
  if (error) throw new Error(error.message)
  return data ?? null
}

export function useVoucherCode(voucherId: string) {
  return useQuery({
    queryKey: queryKeys.voucherCode(voucherId),
    queryFn: () => fetchVoucherCode(voucherId),
    // A voucher's code never changes once issued.
    staleTime: Infinity,
  })
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/**
 * Who did what to this voucher, oldest first. The database filters it by who
 * is asking: a farmer sees issued, answered, redeemed and voided — never a
 * scan, a refusal or the audit hold.
 */
export async function fetchVoucherTimeline(voucherId: string): Promise<AuditEvent[]> {
  if (!isUuid(voucherId)) return []
  const { data, error } = await supabase.rpc('app_voucher_timeline', { p_voucher_id: voucherId })
  if (error) throw new Error(error.message)

  return (data ?? []).map((row) => ({
    occurred_at: row.occurred_at,
    kind: row.kind,
    actor_name: row.actor_name ?? null,
    actor_role: row.actor_role ?? null,
    detail: isRecord(row.detail) ? row.detail : null,
  }))
}

export function useVoucherTimeline(voucherId: string) {
  return useQuery({
    queryKey: queryKeys.voucherTimeline(voucherId),
    queryFn: () => fetchVoucherTimeline(voucherId),
  })
}

/** Who handed the incentive over: the latest `redeemed` event's snapshot name. */
export function redeemedByName(events: AuditEvent[]): string | null {
  const redeemed = events.filter((event) => event.kind === 'redeemed')
  return redeemed.at(-1)?.actor_name ?? null
}

/**
 * `expired` is derived, never stored: the database keeps `issued` and an
 * `expires_at`. Only an uncollected voucher expires — a collected or cancelled
 * one keeps the status that says what happened to it.
 */
export function voucherDisplayStatus(
  voucher: Pick<FarmerVoucher, 'status' | 'expires_at'>,
  now: number,
): VoucherDisplayStatus {
  if (voucher.status === 'issued' && Date.parse(voucher.expires_at) <= now) return 'expired'
  return voucher.status
}
