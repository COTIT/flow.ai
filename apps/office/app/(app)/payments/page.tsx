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
import { Tile } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { PageHead } from "@/components/list"
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
      <ApiError error={refresh.error} fallback={t("common.error")} />
      {list.isPending ? (
        <p role="status">{t("common.loading")}</p>
      ) : list.isError ? (
        <LoadError error={list.error} onRetry={() => void list.refetch()} />
      ) : (
        <>
          {!list.data.data.length ? (
            <Empty title={t("payments.empty")} />
          ) : (
            <div className="flex flex-col gap-3">
              {list.data.data.map((payment) => (
                <Tile
                  key={payment.payment_id}
                  id={`payment-${payment.payment_id}`}
                  title={t(
                    `payments.status.${paymentUncertain(payment) ? "review" : paymentStatusKey(payment.status)}`
                  )}
                  span="col-span-12"
                >
                  <div className="flex flex-wrap items-start justify-between gap-4">
                    <div className="flex min-w-0 flex-col gap-1">
                      <p className="font-semibold tabular-nums">
                        {paymentMoney(payment, locale)}
                      </p>
                      <p className="text-sm text-muted-foreground">
                        {t(
                          payment.kind === "refund"
                            ? "payments.refund"
                            : "payments.collection"
                        )}{" "}
                        ·{" "}
                        {t(
                          payment.environment === "production"
                            ? "payments.production"
                            : "payments.sandbox"
                        )}
                      </p>
                      <code className="text-xs break-all">
                        {payment.payment_id}
                      </code>
                      {canView(payment) &&
                      (payment.invoice_id || payment.credit_note_id) ? (
                        <Link
                          className="inline-flex min-h-11 items-center text-sm underline"
                          href={`/invoices/${payment.credit_note_id ?? payment.invoice_id}`}
                        >
                          {t("payments.openInvoice")}
                        </Link>
                      ) : canView(payment) && payment.order_id ? (
                        <Link
                          className="inline-flex min-h-11 items-center text-sm underline"
                          href={`/orders/${payment.order_id}`}
                        >
                          {t("payments.openOrder")}
                        </Link>
                      ) : null}
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <Button
                        variant="outline"
                        disabled={refresh.isPending || !canView(payment)}
                        onClick={() => refresh.mutate(payment)}
                      >
                        {t("payments.refresh")}
                      </Button>
                      <Button
                        variant="outline"
                        disabled={!canView(payment)}
                        onClick={() => {
                          setSelected(payment)
                          setResolve(false)
                          resolution.reset()
                        }}
                      >
                        {t("payments.details")}
                      </Button>
                    </div>
                  </div>
                </Tile>
              ))}
            </div>
          )}
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
        </>
      )}
      <Dialog
        open={Boolean(selected && canView(selected))}
        onOpenChange={(open) => {
          if (!open) setSelected(null)
        }}
      >
        <DialogContent className="max-h-[90dvh] overflow-y-auto">
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
