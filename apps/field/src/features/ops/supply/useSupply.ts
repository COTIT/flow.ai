import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { farmSupplyList } from '@genesy/api-client/endpoints/farm-supply'
import { supplyChainDispatch, supplyChainList } from '@genesy/api-client/endpoints/supply-chain'
import { supplyLinksGrant, supplyLinksList, supplyLinksRevoke } from '@genesy/api-client/endpoints/supply-links'
import { warehousesList } from '@genesy/api-client/endpoints/warehouses'
import type { LinkResource, SupplyTransferResource } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'

/**
 * The farm side of supply: links this workspace grants, and lot stock it sends over a live one.
 * Every rule (who may grant, which fields, live link, enough free stock) is the API's; its refusal
 * is what the screen shows. The shop side (accept, receive, packing) is the office app's.
 */

export type SupplyLink = LinkResource
export type SupplyTransfer = SupplyTransferResource

/** The API's largest page; lists are read to their last page. */
const PAGE_SIZE = 100

const KEYS = {
  links: ['supplyLinks'],
  transfers: ['supplyTransfers'],
  dispatchOptions: ['supplyDispatchOptions'],
} as const

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
    mutationFn: async ({ toWorkspaceId, fields }: { toWorkspaceId: string; fields: string[] }) =>
      (await supplyLinksGrant(workspaceId(), { to_workspace_id: toWorkspaceId, fields })).data,
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

/**
 * Lots that can be sent are the raw lots of supply batches (the API refuses any other).
 * shortcut: no free stock per lot or warehouse is shown, the API has no read for it; add it when
 * `GET /lots/{lot}/stock` (or lot rows in `GET /products/stocks` for farm staff) is documented.
 */
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

export function useDispatch() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ lotId, linkId, warehouseId, quantity }: DispatchInput) =>
      (await supplyChainDispatch(workspaceId(), lotId, { link_id: linkId, warehouse_id: warehouseId, quantity })).data,
    onSuccess: () => client.invalidateQueries({ queryKey: KEYS.transfers }),
  })
}
