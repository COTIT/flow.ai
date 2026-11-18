"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useSession } from "@genesy/api-client/auth"
import { apiRequest } from "@genesy/api-client/mutator"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Checkbox } from "@genesy/ui/components/checkbox"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { Textarea } from "@genesy/ui/components/textarea"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Field, selectClass } from "@/components/field"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError, NoAccess } from "@/components/states"
import { useRights } from "@/lib/can"
import {
  paymentStatusKey,
  decimalAmount,
  paymentMoney,
  paymentUncertain,
  type PaymentView,
} from "@/lib/payments"

type PaymentList = {
  data: PaymentView[]
  meta: { current_page: number; last_page: number; total: number }
}

export default function PaymentsPage() {
  const workspace = useSession((s) => s.workspaceId)
  const token = useSession((s) => s.token)
  const rights = useRights()
  if (!workspace || !token || !rights.read) return null
  if (!rights.may("invoice.list") && !rights.may("order.list"))
    return <NoAccess />
  return (
    <Payments
      key={`${workspace}:${token}`}
      workspace={workspace}
      admin={rights.may("workspace_admin")}
      may={rights.may}
    />
  )
}

function Payments({
  workspace,
  admin,
  may,
}: {
  workspace: string
  admin: boolean
  may: (right: string) => boolean
}) {
  const t = useTranslations()
  const locale = useLocale()
  const [page, setPage] = React.useState(1)
  const [selected, setSelected] = React.useState<PaymentView | null>(null)
  const [resolve, setResolve] = React.useState(false)
  const returnFocus = React.useRef<HTMLButtonElement | null>(null)
  const queryClient = useQueryClient()
  const path = `/workspace/${workspace}/payments`
  const canView = (payment: PaymentView) =>
    Boolean(
      (payment.invoice_id && may("invoice.view")) ||
      (payment.order_id && may("order.view"))
    )
  const list = useQuery({
    queryKey: [path, page],
    queryFn: ({ signal }) =>
      apiRequest<PaymentList>({
        url: path,
        params: { page, per_page: 25 },
        signal,
      }),
    retry: false,
  })
  const detail = useQuery({
    queryKey: [path, selected?.payment_id],
    queryFn: ({ signal }) =>
      apiRequest<{ data: PaymentView }>({
        url: `${path}/${selected!.payment_id}`,
        signal,
      }),
    enabled: Boolean(selected && canView(selected)),
    retry: false,
  })
  const refresh = useMutation({
    mutationFn: (payment: PaymentView) => {
      if (!canView(payment))
        throw new Error("Payment owner view permission required.")
      return apiRequest({
        url: `${path}/${payment.payment_id}/refresh`,
        method: "POST",
      })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: [path] }),
  })
  const resolution = useMutation({
    mutationFn: (data: { outcome: string; reason: string; evidence: string }) =>
      apiRequest({
        url: `${path}/${selected!.payment_id}/resolve`,
        method: "POST",
        data,
      }),
    onSuccess: () => {
      setResolve(false)
      return queryClient.invalidateQueries({ queryKey: [path] })
    },
  })
  const current = detail.data?.data ?? selected
  return (
    <>
      <PageHead title={t("payments.title")} description={t("payments.intro")} />
      <ListCard
        pager={
          list.data?.data.length ? (
            <nav
              aria-label={t("payments.pages")}
              className="flex items-center justify-between gap-4"
            >
              <Button
                variant="outline"
                disabled={page <= 1}
                onClick={() => setPage(page - 1)}
              >
                {t("payments.previous")}
              </Button>
              <span className="text-sm tabular-nums">
                {page} / {list.data.meta.last_page}
              </span>
              <Button
                variant="outline"
                disabled={page >= list.data.meta.last_page}
                onClick={() => setPage(page + 1)}
              >
                {t("payments.next")}
              </Button>
            </nav>
          ) : null
        }
      >
        {list.isPending ? (
          <ListSkeleton
            columns={[
              { label: t("payments.record.id") },
              { label: t("payments.record.type") },
              { label: t("payments.environment") },
              { label: t("payments.record.status") },
              { label: t("payments.record.amount"), right: true },
            ]}
          />
        ) : list.isError ? (
          <CardState>
            <LoadError error={list.error} onRetry={() => void list.refetch()} />
          </CardState>
        ) : !list.data.data.length ? (
          <CardState>
            <Empty title={t("payments.empty")} />
          </CardState>
        ) : (
          <Table aria-label={t("payments.title")}>
            <TableHeader>
              <TableRow>
                <TableHead>{t("payments.record.id")}</TableHead>
                <TableHead>{t("payments.record.type")}</TableHead>
                <TableHead>{t("payments.environment")}</TableHead>
                <TableHead>{t("payments.record.status")}</TableHead>
                <TableHead className="text-right">
                  {t("payments.record.amount")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {list.data.data.map((payment) => {
                const allowed = canView(payment)
                const status = paymentUncertain(payment)
                  ? "review"
                  : paymentStatusKey(payment.status)
                const openDetails = (button: HTMLButtonElement) => {
                  if (!allowed) return
                  returnFocus.current = button
                  setSelected(payment)
                  setResolve(false)
                  resolution.reset()
                  refresh.reset()
                }
                return (
                  <TableRow
                    key={payment.payment_id}
                    className={allowed ? "cursor-pointer" : undefined}
                    onClick={
                      allowed
                        ? (event) => {
                            const button =
                              event.currentTarget.querySelector("button")
                            if (button) openDetails(button)
                          }
                        : undefined
                    }
                  >
                    <TableCell className="break-all whitespace-normal">
                      <button
                        type="button"
                        className="min-h-11 text-left font-semibold underline-offset-4 hover:underline focus-visible:outline-2 focus-visible:outline-ring disabled:cursor-default disabled:text-muted-foreground"
                        aria-label={`${t("payments.details")}: ${payment.payment_id}`}
                        aria-haspopup="dialog"
                        disabled={!allowed}
                        onClick={(event) => {
                          event.stopPropagation()
                          openDetails(event.currentTarget)
                        }}
                      >
                        {payment.payment_id}
                      </button>
                    </TableCell>
                    <TableCell data-m="sub">
                      {t(
                        payment.kind === "refund"
                          ? "payments.refund"
                          : "payments.collection"
                      )}
                    </TableCell>
                    <TableCell data-m="hide" className="text-muted-foreground">
                      {t(
                        payment.environment === "production"
                          ? "payments.production"
                          : "payments.sandbox"
                      )}
                    </TableCell>
                    <TableCell data-m="badge">
                      <StatusBadge
                        status={status}
                        label={t(`payments.status.${status}`)}
                      />
                    </TableCell>
                    <TableCell
                      data-m="right"
                      className="text-right font-semibold tabular-nums"
                    >
                      {paymentMoney(payment, locale)}
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        )}
      </ListCard>
      <Dialog
        open={Boolean(selected && canView(selected))}
        onOpenChange={(open) => {
          if (!open) setSelected(null)
        }}
      >
        <DialogContent
          data-payment-details=""
          finalFocus={returnFocus}
          className="max-h-[90dvh] overflow-y-auto"
        >
          <DialogHeader>
            <DialogTitle>{t("payments.details")}</DialogTitle>
          </DialogHeader>
          {detail.isPending ? (
            <p role="status">{t("common.loading")}</p>
          ) : detail.isError ? (
            <LoadError
              error={detail.error}
              onRetry={() => void detail.refetch()}
            />
          ) : null}
          {current && canView(current) ? (
            <div className="flex flex-col gap-4">
              <p className="font-semibold">
                {t(
                  `payments.status.${paymentUncertain(current) ? "review" : paymentStatusKey(current.status)}`
                )}
              </p>
              <p className="tabular-nums">{paymentMoney(current, locale)}</p>
              <code className="text-xs break-all">{current.payment_id}</code>
              <div className="flex flex-wrap items-center gap-3">
                <Button
                  variant="outline"
                  disabled={refresh.isPending || !canView(current)}
                  onClick={() => refresh.mutate(current)}
                >
                  {t("payments.refresh")}
                </Button>
                {current.invoice_id || current.credit_note_id ? (
                  <Link
                    className="inline-flex min-h-11 items-center text-sm underline"
                    href={`/invoices/${current.credit_note_id ?? current.invoice_id}`}
                  >
                    {t("payments.openInvoice")}
                  </Link>
                ) : current.order_id ? (
                  <Link
                    className="inline-flex min-h-11 items-center text-sm underline"
                    href={`/orders/${current.order_id}`}
                  >
                    {t("payments.openOrder")}
                  </Link>
                ) : null}
              </div>
              <ApiError error={refresh.error} fallback={t("common.error")} />
              {current.attention_reason ? (
                <Notice variant="warning" title={current.attention_reason} />
              ) : null}
              {(["transactions", "chargebacks"] as const).map((kind) => (
                <section key={kind}>
                  <h3 className="mb-2 font-semibold">
                    {t(`payments.${kind}`)}
                  </h3>
                  {current[kind]?.length ? (
                    <ul className="flex flex-col gap-2">
                      {current[kind]!.map((entry, index) => (
                        <li key={index} className="rounded-xl border p-3">
                          <dl className="flex flex-col gap-1 text-sm">
                            {Object.entries(entry)
                              .filter(
                                ([key, value]) =>
                                  [
                                    "id",
                                    "transaction_id",
                                    "transactionId",
                                    "chargebackId",
                                    "status",
                                    "amount",
                                    "currency",
                                    "type",
                                    "kind",
                                    "created_at",
                                    "created",
                                    "settlement_id",
                                    "settlementId",
                                    "settlementDate",
                                    "settled",
                                  ].includes(key) &&
                                  (typeof value === "string" ||
                                    typeof value === "number" ||
                                    typeof value === "boolean")
                              )
                              .map(([key, value]) => (
                                <div
                                  key={key}
                                  className="flex flex-wrap justify-between gap-2"
                                >
                                  <dt>{t(`payments.record.${key}`)}</dt>
                                  <dd className="max-w-full break-all" data-raw>
                                    {typeof value === "boolean"
                                      ? t(
                                          value ? "payments.yes" : "payments.no"
                                        )
                                      : key === "amount"
                                        ? decimalAmount(
                                            value as string | number,
                                            current.decimals,
                                            locale
                                          )
                                        : String(value)}
                                  </dd>
                                </div>
                              ))}
                          </dl>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      {t("payments.noRecords")}
                    </p>
                  )}
                </section>
              ))}
              {admin &&
              !resolve &&
              (current.status === "unknown" ||
                (current.kind === "payment" && current.status === "open")) ? (
                <Button variant="outline" onClick={() => setResolve(true)}>
                  {t("payments.resolve")}
                </Button>
              ) : null}
              {resolve && admin ? (
                <form
                  className="flex flex-col gap-4"
                  onSubmit={(event) => {
                    event.preventDefault()
                    const form = new FormData(event.currentTarget)
                    resolution.mutate({
                      outcome: String(form.get("outcome")),
                      reason: String(form.get("reason")).trim(),
                      evidence: String(form.get("evidence")).trim(),
                    })
                  }}
                >
                  <Notice variant="warning" title={t("payments.resolveHint")} />
                  <Field id="resolution-outcome" label={t("payments.outcome")}>
                    <select name="outcome" className={selectClass}>
                      <option value="not_accepted">
                        {t("payments.notAccepted")}
                      </option>
                      {current.kind === "refund" &&
                      current.status === "unknown" ? (
                        <option value="refund_confirmed">
                          {t("payments.refundConfirmed")}
                        </option>
                      ) : null}
                    </select>
                  </Field>
                  <Field id="resolution-reason" label={t("payments.reason")}>
                    <Input name="reason" required maxLength={1000} />
                  </Field>
                  <Field
                    id="resolution-evidence"
                    label={t("payments.evidence")}
                  >
                    <Textarea name="evidence" required maxLength={2000} />
                  </Field>
                  <label className="flex min-h-11 items-start gap-3 text-sm">
                    <Checkbox
                      required
                      className="mt-1 size-5 shrink-0 after:-inset-3"
                    />
                    {t("payments.attest")}
                  </label>
                  <ApiError
                    error={resolution.error}
                    fallback={t("common.error")}
                  />
                  <Button type="submit" disabled={resolution.isPending}>
                    {t("payments.resolve")}
                  </Button>
                </form>
              ) : null}
            </div>
          ) : null}
        </DialogContent>
      </Dialog>
    </>
  )
}
