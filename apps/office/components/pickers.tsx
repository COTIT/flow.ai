"use client"

import { useSession } from "@verzo/api-client/auth"
import {
  useAccountingAccountsGet,
  useAccountingAccountsList,
} from "@verzo/api-client/endpoints/accounting"
import {
  useCurrenciesGet,
  useCurrenciesList,
} from "@verzo/api-client/endpoints/currencies"
import { useProductCategoriesList } from "@verzo/api-client/endpoints/product-categories"
import {
  useRelationsGet,
  useRelationsList,
} from "@verzo/api-client/endpoints/relations"
import {
  useWarehousesGet,
  useWarehousesList,
} from "@verzo/api-client/endpoints/warehouses"
import type {
  AccountResource,
  RelationResource,
} from "@verzo/api-client/model"

import {
  PAGE_SIZE,
  SearchPicker,
  type Option,
  type UseLabel,
  type UseOptions,
} from "@/components/search-picker"

type Props = Omit<
  React.ComponentProps<typeof SearchPicker>,
  "useOptions" | "useLabel"
> & {
  /** The chosen id's label when the caller already has it (skips the single GET). */
  knownLabel?: string
}

const useWs = () => useSession((s) => s.workspaceId) ?? ""
const on = (ws: string, more = true) => ({
  query: { enabled: Boolean(ws) && more },
})
const page = <T,>(
  r: {
    data?: { data: T[]; meta: { total: number } }
    isLoading: boolean
    isError: boolean
  },
  option: (x: T) => Option
) => ({
  options: r.data?.data.map(option),
  total: r.data?.meta.total,
  size: PAGE_SIZE,
  isLoading: r.isLoading,
  isError: r.isError,
})

export const accountLabel = (a: Pick<AccountResource, "code" | "name">) =>
  `${a.code} ${a.name}`

/** Ledger accounts by code or name; `filter` hides accounts (e.g. inactive) from the choices. */
export function AccountPicker({
  filter,
  knownLabel,
  ...props
}: Props & { filter?: (a: AccountResource) => boolean }) {
  const useOptions: UseOptions = (q, p) => {
    const ws = useWs()
    const r = useAccountingAccountsList(
      ws,
      { q: q || undefined, page: String(p), page_size: String(PAGE_SIZE) },
      on(ws)
    )
    const shown = r.data && {
      ...r.data,
      data: filter ? r.data.data.filter(filter) : r.data.data,
    }
    return page({ ...r, data: shown }, (a) => ({
      id: a.id,
      label: accountLabel(a),
    }))
  }
  const useLabel: UseLabel = (id) => {
    const ws = useWs()
    const r = useAccountingAccountsGet(
      ws,
      id,
      on(ws, Boolean(id) && !knownLabel)
    )
    return knownLabel ?? (r.data?.data ? accountLabel(r.data.data) : undefined)
  }
  return <SearchPicker {...props} useOptions={useOptions} useLabel={useLabel} />
}

/** Currencies by name or ISO code. */
export function CurrencyPicker({ knownLabel, ...props }: Props) {
  const useOptions: UseOptions = (q, p) => {
    const ws = useWs()
    const r = useCurrenciesList(
      ws,
      { q: q || undefined, page: p, page_size: PAGE_SIZE },
      on(ws)
    )
    return page(r, (c) => ({ id: c.id, label: c.asset_type, hint: c.name }))
  }
  const useLabel: UseLabel = (id) => {
    const ws = useWs()
    const r = useCurrenciesGet(ws, id, on(ws, Boolean(id) && !knownLabel))
    return knownLabel ?? r.data?.data.asset_type
  }
  return <SearchPicker {...props} useOptions={useOptions} useLabel={useLabel} />
}

/** Product categories by name; values are `product_category_id` (what tax profiles store).
 * shortcut: the chosen label comes from the caller (`knownLabel`), since the single GET reads by
 * the category row id; read by product_category_id if a screen needs labels beyond its list. */
export function CategoryPicker({ knownLabel, ...props }: Props) {
  const useOptions: UseOptions = (q, p) => {
    const ws = useWs()
    const r = useProductCategoriesList(
      ws,
      { q: q || undefined, page: p, page_size: PAGE_SIZE },
      on(ws)
    )
    return page(r, (c) => ({ id: c.product_category_id, label: c.name }))
  }
  const useLabel: UseLabel = () => knownLabel
  return <SearchPicker {...props} useOptions={useOptions} useLabel={useLabel} />
}

/** Warehouses at every level by name or alias. */
export function WarehousePicker({ knownLabel, ...props }: Props) {
  const useOptions: UseOptions = (q, p) => {
    const ws = useWs()
    const r = useWarehousesList(
      ws,
      {
        q: q || undefined,
        page: p,
        page_size: PAGE_SIZE,
        roots_only: false,
      },
      on(ws)
    )
    return page(r, (w) => ({ id: w.id, label: w.name }))
  }
  const useLabel: UseLabel = (id) => {
    const ws = useWs()
    const r = useWarehousesGet(ws, id, on(ws, Boolean(id) && !knownLabel))
    return knownLabel ?? r.data?.data.name
  }
  return <SearchPicker {...props} useOptions={useOptions} useLabel={useLabel} />
}

export const relationLabel = (
  r: Pick<RelationResource, "company_name" | "alias" | "relation_nr" | "id">
) => r.company_name ?? r.alias ?? r.relation_nr ?? r.id

/** Relations (customers and suppliers) by the relations search. */
export function RelationPicker({ knownLabel, ...props }: Props) {
  const useOptions: UseOptions = (q, p) => {
    const ws = useWs()
    const r = useRelationsList(
      ws,
      { q: q || undefined, page: p, limit: PAGE_SIZE },
      on(ws)
    )
    return page(r, (x) => ({ id: x.id, label: relationLabel(x) }))
  }
  const useLabel: UseLabel = (id) => {
    const ws = useWs()
    const r = useRelationsGet(
      ws,
      id,
      undefined,
      on(ws, Boolean(id) && !knownLabel)
    )
    return knownLabel ?? (r.data?.data ? relationLabel(r.data.data) : undefined)
  }
  return <SearchPicker {...props} useOptions={useOptions} useLabel={useLabel} />
}
