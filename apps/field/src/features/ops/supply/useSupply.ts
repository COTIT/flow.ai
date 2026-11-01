import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { farmSupplyList } from '@genesy/api-client/endpoints/farm-supply'
import { lotsStock } from '@genesy/api-client/endpoints/lots'
import { supplyChainDispatch, supplyChainList } from '@genesy/api-client/endpoints/supply-chain'
import { supplyLinksGrant, supplyLinksList, supplyLinksRevoke } from '@genesy/api-client/endpoints/supply-links'
import { warehousesList } from '@genesy/api-client/endpoints/warehouses'
import type { LinkResource, SupplyTransferResource } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'

/**
 * The farm side of supply: links this workspace grants, and lot stock it sends over a live one.
 * Every rule (who may grant, which fields, live link, enough free stock) is the API's; its refusal
 * is what the screen shows. The shop side (accept, receive, packing) is the office app's.
 *
 * Grant and dispatch carry the form's `Idempotency-Key`: a retry after a lost reply replays the
 * first answer instead of granting or sending twice.
 */

export type SupplyLink = LinkResource
export type SupplyTransfer = SupplyTransferResource

/** The API's largest page; lists are read to their last page. */
const PAGE_SIZE = 100

const KEYS = {
  links: ['supplyLinks'],
  transfers: ['supplyTransfers'],
  dispatchOptions: ['supplyDispatchOptions'],
  lotStock: (lotId: string) => ['supplyLotStock', lotId],
} as const

/** One key per form attempt, made by the screen; sent as the `Idempotency-Key` header. */
const idempotent = (key: string) => ({ headers: { 'Idempotency-Key': key } })

async function readAll<T>(
  read: (params: { page: number; page_size: number }) => Promise<{ data: T[]; meta: { last_page: number } }>,
): Promise<T[]> {
  const rows: T[] = []
  for (let page = 1, last = 1; page <= last; page++) {
    const reply = await read({ page, page_size: PAGE_SIZE })
    rows.push(...reply.data)
    last = reply.meta.last_page
  }
  return rows
}

export function useSupplyLinks() {
  return useQuery({
    queryKey: KEYS.links,
    queryFn: async () =>
      (await readAll((p) => supplyLinksList(workspaceId(), p))).filter((l) => l.direction === 'outgoing'),
  })
}

export function useGrantLink() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ toWorkspaceId, fields, idempotencyKey }: { toWorkspaceId: string; fields: string[]; idempotencyKey: string }) =>
      (await supplyLinksGrant(workspaceId(), { to_workspace_id: toWorkspaceId, fields }, idempotent(idempotencyKey))).data,
    onSuccess: () => client.invalidateQueries({ queryKey: KEYS.links }),
  })
}

export function useRevokeLink() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: (linkId: string) => supplyLinksRevoke(workspaceId(), linkId),
    onSuccess: () => client.invalidateQueries({ queryKey: KEYS.links }),
  })
}

export function useSupplyTransfers() {
  return useQuery({
    queryKey: KEYS.transfers,
    queryFn: async () =>
      (await readAll((p) => supplyChainList(workspaceId(), p))).filter((t) => t.direction === 'outgoing'),
  })
}

/** Lots that can be sent are the raw lots of supply batches (the API refuses any other). */
export function useDispatchOptions() {
  return useQuery({
    queryKey: KEYS.dispatchOptions,
    queryFn: async () => {
      const workspace = workspaceId()
      const [batches, warehouses] = await Promise.all([
        readAll((p) => farmSupplyList(workspace, p)),
        readAll((p) => warehousesList(workspace, p)),
      ])
      return {
        lots: batches.map((b) => ({ id: b.lot.id, code: b.lot.code })),
        warehouses: warehouses.map((w) => ({ id: w.id, name: w.name })),
      }
    },
  })
}

export interface DispatchInput {
  lotId: string
  linkId: string
  warehouseId: string
  /** Decimal text as typed; the API checks it. Never a float. */
  quantity: string
}

/** A lot's stock per warehouse (level, reserved, available): display only, the API still refuses an over-send. */
export function useLotStock(lotId: string) {
  return useQuery({
    queryKey: KEYS.lotStock(lotId),
    enabled: lotId !== '',
    queryFn: async () => (await lotsStock(workspaceId(), lotId)).data,
  })
}

export function useDispatch() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ lotId, linkId, warehouseId, quantity, idempotencyKey }: DispatchInput & { idempotencyKey: string }) =>
      (
        await supplyChainDispatch(
          workspaceId(),
          lotId,
          { link_id: linkId, warehouse_id: warehouseId, quantity },
          idempotent(idempotencyKey),
        )
      ).data,
    onSuccess: (_data, { lotId }) =>
      Promise.all([
        client.invalidateQueries({ queryKey: KEYS.transfers }),
        client.invalidateQueries({ queryKey: KEYS.lotStock(lotId) }),
      ]),
  })
}
