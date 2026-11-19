"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useSupplierPaymentsList } from "@genesy/api-client/endpoints/supplier-payments"
import { buttonVariants } from "@genesy/ui/components/button"
import { TableCell, TableRow } from "@genesy/ui/components/table"
import { formatDay } from "@genesy/ui/lib/format"
import { PageHead } from "@/components/list"
import { PayablesList, usePayWord } from "@/components/payables"
import { StatusBadge } from "@/components/status-badge"
import { useCan } from "@/lib/can"

import { showMoney } from "../purchases/shared"

/** Supplier payments and refunds received (Template List, as /purchases): supplier filter, server totals, pager. */
export default function SupplierPaymentsPage({
  searchParams,
}: {
  searchParams: Promise<{ supplier?: string; page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = usePayWord()
  const mayCreate = useCan("supplierpayment.create")
  const { supplier, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const payments = useSupplierPaymentsList(
    workspaceId,
    { supplier_id: supplier || undefined, page: page || undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  return (
    <>
      <PageHead
        eyebrow={t("shell.buy")}
        title={t("payables.payments.title")}
        actions={
          mayCreate ? (
            <Link href="/supplier-payments/new" className={buttonVariants()}>
              {t("payables.payments.new")}
            </Link>
          ) : null
        }
      />
      <PayablesList
        supplier={supplier}
        query={payments}
        count={(total) => t("payables.payments.count", { total })}
        empty={t("payables.payments.empty")}
        columns={[
          { label: t("payables.number") },
          { label: t("payables.supplier") },
          { label: t("payables.state") },
          { label: t("payables.payments.date") },
          { label: t("payables.amount"), right: true },
        ]}
        row={(p) => (
          <TableRow key={p.id}>
            <TableCell>
              <Link className="font-semibold underline-offset-4 hover:underline" href={`/supplier-payments/${p.id}`}>
                {t("payables.payments.detailTitle", { number: p.number })}
              </Link>
              <span className="block text-xs text-muted-foreground">{word("directions", p.direction)}</span>
            </TableCell>
            <TableCell data-m="sub">{p.supplier.name ?? ""}</TableCell>
            <TableCell data-m="badge">
              <StatusBadge status={p.state} label={word("states", p.state)} />
            </TableCell>
            <TableCell data-m="hide" className="text-muted-foreground">
              {formatDay(p.date, locale)}
            </TableCell>
            <TableCell data-m="right" className="text-right font-semibold tabular-nums">
              {showMoney(p.amount, p.currency, locale)}
            </TableCell>
          </TableRow>
        )}
      />
    </>
  )
}
