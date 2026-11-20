"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useSession } from "@verzo/api-client/auth"
import { apiRequest } from "@verzo/api-client/mutator"
import { RefreshCw } from "lucide-react"
import { Badge } from "@verzo/ui/components/badge"
import { ApiError } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { Checkbox } from "@verzo/ui/components/checkbox"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@verzo/ui/components/dialog"
import { Input } from "@verzo/ui/components/input"
import { Notice } from "@verzo/ui/components/notice"
import { Textarea } from "@verzo/ui/components/textarea"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@verzo/ui/components/table"
import { Field, selectClass } from "@/components/field"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { Empty, LoadError, NoAccess } from "@/components/states"
import { useRights } from "@/lib/can"
import {
  paymentStatusKey,
  paymentTone,
  providerStatusKey,
  providerMoney,
  paymentTimestamp,
  paymentMoney,
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
  const reconcileRefund = Boolean(
    detail.isSuccess &&
    current?.kind === "payment" &&
    current.status === "paid" &&
    current.attention &&
    current.attention_reason ===
      "External refund or chargeback observation requires reconciliation."
  )
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
                        {t(
                          payment.kind === "refund"
                            ? "payments.refund"
                            : "payments.collection"
                        )}
                      </button>
                    </TableCell>
                    <TableCell data-m="sub" className="text-muted-foreground">
                      {t(
                        payment.environment === "production"
                          ? "payments.production"
                          : payment.environment === "sandbox"
                            ? "payments.sandbox"
                            : "payments.environmentUnavailable"
                      )}
                    </TableCell>
                    <TableCell data-m="badge">
                      <PaymentBadge payment={payment} />
                    </TableCell>
                    <TableCell
                      data-m="right"
                      className="text-right font-semibold tabular-nums"
                    >
                      {paymentMoney(payment, locale)}{" "}
                      <span className="font-normal text-muted-foreground">
                        {payment.currency}
                      </span>
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
          {detail.isSuccess && current && canView(current) ? (
            <div className="flex flex-col gap-6">
              <div className="rounded-2xl bg-muted/50 p-4">
                <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                  <p className="text-sm text-muted-foreground">
                    {t(
                      current.kind === "refund"
                        ? "payments.refund"
                        : "payments.collection"
                    )}
                  </p>
                  <PaymentBadge payment={current} />
                </div>
                <p className="text-3xl font-semibold tabular-nums">
                  {paymentMoney(current, locale)}{" "}
                  <span className="text-base font-normal text-muted-foreground">
                    {current.currency}
                  </span>
                </p>
                <div className="mt-3 flex flex-wrap items-center gap-3">
                  <Badge
                    variant={
                      current.environment === "production" ? "slate" : "gold"
                    }
                  >
                    {t(
                      current.environment === "production"
                        ? "payments.production"
                        : current.environment === "sandbox"
                          ? "payments.sandbox"
                          : "payments.environmentUnavailable"
                    )}
                  </Badge>
                  <OwnerLink payment={current} allowed={canView(current)} />
                  <Button
                    variant="outline"
                    className="min-h-11"
                    disabled={refresh.isPending || !canView(current)}
                    onClick={() => refresh.mutate(current)}
                  >
                    <RefreshCw
                      aria-hidden="true"
                      className={`size-4 ${refresh.isPending ? "motion-safe:animate-spin" : ""}`}
                    />
                    {t("payments.refresh")}
                  </Button>
                </div>
              </div>
              <ApiError error={refresh.error} fallback={t("common.error")} />
              {current.attention_reason ? (
                <Notice variant="warning" title={t("payments.status.review")}>
                  {current.attention_reason}
                </Notice>
              ) : null}
              {(["transactions", "chargebacks"] as const).map((kind) => {
                const records =
                  kind === "transactions"
                    ? [...(current.transactions ?? [])].sort(
                        (a, b) =>
                          Number(b.id === current.captured_transaction_id) -
                          Number(a.id === current.captured_transaction_id)
                      )
                    : current.chargebacks
                return (
                  <section
                    key={kind}
                    aria-label={t(`payments.${kind}`)}
                    className="flex flex-col gap-3"
                  >
                    <h3 className="font-semibold">{t(`payments.${kind}`)}</h3>
                    {kind === "transactions" && current.transactions?.length ? (
                      <p className="text-sm text-muted-foreground">
                        {t("payments.transactionHint")}
                      </p>
                    ) : null}
                    {records?.length ? (
                      <ul className="flex flex-col gap-3">
                        {records!.map((entry, index) => (
                          <ProviderRecord
                            key={index}
                            entry={entry}
                            index={index}
                            payment={current}
                            kind={kind}
                          />
                        ))}
                      </ul>
                    ) : (
                      <p className="rounded-xl bg-muted/40 p-4 text-sm text-muted-foreground">
                        {t(
                          kind === "chargebacks"
                            ? "payments.noChargebacks"
                            : "payments.noRecords"
                        )}
                      </p>
                    )}
                  </section>
                )
              })}
              <details className="rounded-xl border px-4">
                <summary className="min-h-11 cursor-pointer py-3 text-sm font-medium focus-visible:outline-2 focus-visible:outline-ring">
                  {t("payments.supportReferences")}
                </summary>
                <dl className="pb-4 text-sm">
                  <dt className="mb-1 text-muted-foreground">
                    {t("payments.paymentReference")}
                  </dt>
                  <dd>
                    <code className="break-all select-all">
                      {current.payment_id}
                    </code>
                  </dd>
                </dl>
              </details>
              {admin &&
              !resolve &&
              (current.status === "unknown" ||
                (current.kind === "payment" && current.status === "open") ||
                reconcileRefund) ? (
                <Button variant="outline" onClick={() => setResolve(true)}>
                  {t("payments.resolve")}
                </Button>
              ) : null}
              {resolve &&
              admin &&
              (current.status !== "paid" || reconcileRefund) ? (
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
                  <Notice
                    variant="warning"
                    title={t(
                      reconcileRefund
                        ? "payments.refundReconciledHint"
                        : "payments.resolveHint"
                    )}
                  />
                  <Field id="resolution-outcome" label={t("payments.outcome")}>
                    <select name="outcome" className={selectClass}>
                      {reconcileRefund ? (
                        <option value="refund_reconciled">
                          {t("payments.refundReconciled")}
                        </option>
                      ) : (
                        <option value="not_accepted">
                          {t("payments.notAccepted")}
                        </option>
                      )}
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

function PaymentBadge({ payment }: { payment: PaymentView }) {
  const t = useTranslations()
  const status = paymentStatusKey(payment.status)
  return (
    <div className="flex flex-wrap gap-2">
      <Badge
        variant={paymentTone(status)}
        className="h-auto min-h-6 px-2.5 py-1 text-sm"
      >
        {t(`payments.status.${status}`)}
      </Badge>
      {payment.attention && status !== "review" ? (
        <Badge variant="gold" className="h-auto min-h-6 px-2.5 py-1 text-sm">
          {t("payments.status.review")}
        </Badge>
      ) : null}
    </div>
  )
}

function OwnerLink({
  payment,
  allowed,
}: {
  payment: PaymentView
  allowed: boolean
}) {
  const t = useTranslations()
  if (!allowed) return null
  const invoice = payment.credit_note_id ?? payment.invoice_id
  if (!invoice && !payment.order_id) return null
  return (
    <Link
      className="inline-flex min-h-11 items-center text-sm underline underline-offset-4 focus-visible:outline-2 focus-visible:outline-ring"
      href={invoice ? `/invoices/${invoice}` : `/orders/${payment.order_id}`}
    >
      {t(invoice ? "payments.openInvoice" : "payments.openOrder")}
    </Link>
  )
}

function ProviderRecord({
  entry,
  index,
  payment,
  kind,
}: {
  entry: Record<string, unknown>
  index: number
  payment: PaymentView
  kind: "transactions" | "chargebacks"
}) {
  const t = useTranslations()
  const locale = useLocale()
  const status = providerStatusKey(entry.status)
  const amount = providerMoney(
    entry.amount,
    entry.currency,
    payment.decimals,
    locale
  )
  const recorded = paymentTimestamp(entry.created ?? entry.created_at, locale)
  const settlementDate = paymentTimestamp(entry.settlementDate, locale)
  const id =
    kind === "transactions"
      ? (entry.id ?? entry.transaction_id ?? entry.transactionId)
      : (entry.chargebackId ?? entry.id)
  const confirmed =
    kind === "transactions" &&
    payment.kind !== "refund" &&
    typeof id === "string" &&
    id === payment.captured_transaction_id
  return (
    <li className="flex flex-col gap-3 rounded-xl border p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-sm text-muted-foreground">
            {t(
              kind === "transactions"
                ? "payments.transactionNumber"
                : "payments.chargebackNumber",
              { number: index + 1 }
            )}
          </p>
          <p className="mt-1 text-lg font-semibold tabular-nums">
            {amount ?? t("payments.unavailable")}
            {typeof entry.currency === "string" && amount ? (
              <span className="text-sm font-normal text-muted-foreground">
                {" "}
                {entry.currency}
              </span>
            ) : null}
          </p>
        </div>
        <Badge
          variant={paymentTone(status)}
          className="h-auto min-h-6 px-2.5 py-1 text-sm"
        >
          {t(`payments.providerStatus.${status}`)}
        </Badge>
      </div>
      {confirmed ? (
        <p className="text-sm font-medium text-(--tone-success)">
          {t("payments.confirmedCapture")}
        </p>
      ) : null}
      <dl className="grid gap-3 text-sm">
        <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-1">
          <dt className="text-muted-foreground">
            {t("payments.record.created")}
          </dt>
          <dd className="text-right">
            {recorded ? (
              <>
                <time dateTime={recorded.iso}>{recorded.text}</time>
                {recorded.providerTime ? (
                  <span className="mt-1 block text-xs text-muted-foreground">
                    {t("payments.providerTime")}
                  </span>
                ) : null}
              </>
            ) : (
              t("payments.unavailable")
            )}
          </dd>
        </div>
        {kind === "transactions" ? (
          <div className="flex flex-wrap items-center justify-between gap-2">
            <dt className="text-muted-foreground">
              {t("payments.settlement")}
            </dt>
            <dd>
              <Badge variant={entry.settled === true ? "teal" : "slate"}>
                {t(
                  entry.settled === true
                    ? "payments.status.settled"
                    : entry.settled === false
                      ? "payments.notSettled"
                      : "payments.unavailable"
                )}
              </Badge>
            </dd>
          </div>
        ) : null}
        {kind === "transactions" && settlementDate ? (
          <div className="flex flex-wrap justify-between gap-2">
            <dt className="text-muted-foreground">
              {t("payments.record.settlementDate")}
            </dt>
            <dd>
              <time dateTime={settlementDate.iso}>{settlementDate.text}</time>
              {settlementDate.providerTime ? (
                <span className="block text-xs text-muted-foreground">
                  {t("payments.providerTime")}
                </span>
              ) : null}
            </dd>
          </div>
        ) : null}
      </dl>
      {typeof id === "string" ? (
        <details className="border-t pt-1">
          <summary className="min-h-11 cursor-pointer py-3 text-sm text-muted-foreground focus-visible:outline-2 focus-visible:outline-ring">
            {t("payments.supportReferences")}
          </summary>
          <dl className="flex flex-col gap-2 text-sm">
            <div>
              <dt className="text-muted-foreground">
                {t(
                  kind === "transactions"
                    ? "payments.record.transactionId"
                    : "payments.record.chargebackId"
                )}
              </dt>
              <dd className="mt-1">
                <code className="break-all select-all">{id}</code>
              </dd>
            </div>
            {typeof entry.settlementId === "string" ? (
              <div>
                <dt className="text-muted-foreground">
                  {t("payments.record.settlementId")}
                </dt>
                <dd className="mt-1">
                  <code className="break-all select-all">
                    {entry.settlementId}
                  </code>
                </dd>
              </div>
            ) : null}
          </dl>
        </details>
      ) : null}
    </li>
  )
}
