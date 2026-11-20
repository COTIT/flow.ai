"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import { usePurchaseReturnsList } from "@verzo/api-client/endpoints/purchase-returns"
import { TableCell, TableRow } from "@verzo/ui/components/table"
import { formatDay } from "@verzo/ui/lib/format"
import { PageHead } from "@/components/list"
import { PayablesList, usePayWord } from "@/components/payables"
import { StatusBadge } from "@/components/status-badge"

/** Purchase returns (Template List, as /purchases): supplier filter, pager; new returns start from a receipt. */
export default function PurchaseReturnsPage({
  searchParams,
}: {
  searchParams: Promise<{ supplier?: string; page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = usePayWord()
  const { supplier, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const returns = usePurchaseReturnsList(
    workspaceId,
    { supplier_id: supplier || undefined, page: page || undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  return (
    <>
      <PageHead
        eyebrow={t("shell.buy")}
        title={t("payables.returns.title")}
        description={t("payables.returns.hint")}
      />
      <PayablesList
        supplier={supplier}
        query={returns}
        count={(total) => t("payables.returns.count", { total })}
        empty={t("payables.returns.empty")}
        columns={[
          { label: t("payables.number") },
          { label: t("payables.supplier") },
          { label: t("payables.state") },
          { label: t("payables.accountingDate") },
          { label: t("payables.reason") },
        ]}
        row={(p) => (
          <TableRow key={p.id}>
            <TableCell>
              <Link className="font-semibold underline-offset-4 hover:underline" href={`/purchase-returns/${p.id}`}>
                {t("payables.returns.detailTitle", { number: p.number })}
              </Link>
              <span className="block text-xs text-muted-foreground">{word("purposes", p.purpose)}</span>
            </TableCell>
            <TableCell data-m="sub">{p.supplier.name ?? ""}</TableCell>
            <TableCell data-m="badge">
              <StatusBadge status={p.state} label={word("states", p.state)} />
            </TableCell>
            <TableCell data-m="hide" className="text-muted-foreground">
              {formatDay(p.accounting_date, locale)}
            </TableCell>
            <TableCell data-m="hide" className="max-w-[240px] truncate">
              {p.reason}
            </TableCell>
          </TableRow>
        )}
      />
    </>
  )
}
