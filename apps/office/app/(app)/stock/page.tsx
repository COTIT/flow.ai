"use client"

import * as React from "react"

import { useSession } from "@genesy/api-client/auth"
import { useStockList } from "@genesy/api-client/endpoints/stock"

import { StockTable } from "./stock-table"

export default function StockPage() {
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [page, setPage] = React.useState(1)
  const stock = useStockList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  return <StockTable view="all" query={stock} onPage={setPage} />
}
