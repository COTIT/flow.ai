import { warehousesList } from "@verzo/api-client/endpoints/warehouses"
import type {
  PaginatedWarehouseResource,
  WarehouseResource,
} from "@verzo/api-client/model"

export const allWarehouses = { roots_only: false, page_size: 100 } as const

/** Find a known warehouse past the picker's first page without needing warehouse.get. */
export async function findWarehouse(
  workspace: string,
  first: PaginatedWarehouseResource,
  matches: (warehouse: WarehouseResource) => boolean,
  signal?: AbortSignal
) {
  let result = first
  for (let page = 2; ; page++) {
    const warehouse = result.data.find(matches)
    if (warehouse || page > result.meta.last_page) return warehouse ?? null
    result = await warehousesList(
      workspace,
      { ...allWarehouses, page },
      undefined,
      signal
    )
  }
}
