"use client"

import * as React from "react"

import { useSession } from "@genesy/api-client/auth"
import { useStockNegative } from "@genesy/api-client/endpoints/stock"

import { StockTable } from "../stock-table"

export default function NegativeStockPage() {
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [page, setPage] = React.useState(1)
  const stock = useStockNegative(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  return <StockTable view="negative" query={stock} onPage={setPage} />
}
