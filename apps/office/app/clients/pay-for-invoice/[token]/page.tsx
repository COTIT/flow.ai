"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useInvoicesPublic } from "@genesy/api-client/endpoints/invoices"
import { apiRequest } from "@genesy/api-client/mutator"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { formatDate, formatQty } from "@genesy/ui/lib/format"
import { Bento, DetailSkeleton, Tile } from "@/components/bento"
import { Field } from "@/components/field"
import { LoadError } from "@/components/states"
import {
  hostedPaymentUrl,
  decimalAmount,
  paymentStatusKey,
  paymentUncertain,
  type PaymentView,
} from "@/lib/payments"

export default function InvoicePaymentPage({
  params,
}: {
  params: Promise<{ token: string }>
}) {
  const { token } = React.use(params)
  return <PublicInvoice key={token} token={token} />
}

function PublicInvoice({ token }: { token: string }) {
  const t = useTranslations()
  const locale = useLocale()
  const queryClient = useQueryClient()
  const path = `/invoices/fetch-from-hashed/${encodeURIComponent(token)}`
  const invoice = useInvoicesPublic(token, { query: { retry: false } })
  const refetchInvoice = invoice.refetch
  const payment = useQuery({
    queryKey: [path, "payment"],
    queryFn: ({ signal }) =>
      apiRequest<{ data: PaymentView | null }>({
        url: `${path}/payment`,
        signal,
      }),
    enabled: invoice.isSuccess,
    retry: false,
    refetchInterval: (query) => {
      const current = query.state.data?.data
      return current &&
        ![
          "paid",
          "refunded",
          "cancelled",
          "canceled",
          "expired",
          "failed",
        ].includes(current.status)
        ? 10000
        : false
    },
  })
  const [uncertain, setUncertain] = React.useState(false)
  const [setupRequired, setSetupRequired] = React.useState(false)
  const create = useMutation({
    mutationFn: (data: {
      country: string
      payer_name: string
      payer_email?: string
    }) =>
      apiRequest<{ data: PaymentView }>({
        url: `${path}/payment`,
        method: "POST",
        data,
      }),
    onSuccess: (data) => {
      queryClient.setQueryData([path, "payment"], data)
      void payment.refetch()
    },
    onError: (error) => {
      // A transport failure can follow an accepted payment: check before allowing another attempt.
      const response = (
        error as { response?: { status?: number; data?: { code?: string } } }
      ).response
      if (
        response?.status === 503 &&
        response.data?.code === "payment_setup_required"
      ) {
        setSetupRequired(true)
        void refetchInvoice()
      } else if (!response || (response.status ?? 500) >= 500)
        setUncertain(true)
      void payment.refetch()
    },
  })
  React.useEffect(() => {
    if (payment.data?.data?.status === "paid") void refetchInvoice()
  }, [payment.data?.data?.status, refetchInvoice])
  if (invoice.isPending)
    return (
      <main className="mx-auto max-w-5xl p-4 md:p-8">
        <DetailSkeleton />
      </main>
    )
  if (invoice.isError)
    return (
      <main className="mx-auto max-w-3xl p-6">
        <LoadError
          error={invoice.error}
          onRetry={() => void invoice.refetch()}
        />
      </main>
    )
  const inv = invoice.data.data
  const current = payment.data?.data ?? create.data?.data
  const checkout = hostedPaymentUrl(current?.checkout_url)
  const review =
    (!current && uncertain) || Boolean(current && paymentUncertain(current))
  const currency = inv.relationships.currency
  const money = (amount: number | string) =>
    `${currency?.symbol_prefix ?? ""}${decimalAmount(amount, currency?.decimals ?? 2, locale)}${currency?.symbol_suffix ?? ""}`
  const company = inv.company_details
  const account = (company.account ?? {}) as Record<string, unknown>
  const text = (value: unknown) => (typeof value === "string" ? value : "")
  const errors = fieldErrors(create.error)
  const payable = !inv.is_credit && inv.balance.open > 0
  const resumable =
    current?.status === "open" && Boolean(checkout) && !review && payable
  return (
    <main className="mx-auto flex w-full max-w-5xl flex-col gap-6 p-4 md:p-8">
      <header className="flex flex-col gap-2">
        <p className="text-sm text-muted-foreground">
          {text(company.company_name) || text(company.name)}
        </p>
        <h1 className="text-2xl font-semibold">
          {t("invoices.detailTitle", { number: inv.invoice_nr ?? "—" })}
        </h1>
        {inv.reference ? (
          <p className="text-sm break-words">
            {t("invoices.reference")}: {inv.reference}
          </p>
        ) : null}
        {inv.due_date ? (
          <p className="text-sm text-muted-foreground">
            {t("invoices.due")}: {formatDate(inv.due_date, locale)}
          </p>
        ) : null}
      </header>
      <Bento>
        <Tile id="public-invoice" title={t("invoices.lines")} span="col-span-7">
          <ul className="flex flex-col gap-4">
            {inv.items.map((item) => (
              <li key={item.id} className="flex justify-between gap-4 text-sm">
                <span className="min-w-0 break-words">
                  {item.title} × {formatQty(item.quantity, locale)}
                </span>
                <span className="shrink-0 tabular-nums">
                  {money(item.total_incl)}
                </span>
              </li>
            ))}
          </ul>
          <dl className="flex flex-col gap-2 border-t pt-4 text-sm">
            {(["total", "paid", "open"] as const).map((key) => (
              <div key={key} className="flex justify-between gap-4">
                <dt>{t(`invoices.${key}`)}</dt>
                <dd className="font-semibold tabular-nums">
                  {money(inv.balance[key])}
                </dd>
              </div>
            ))}
          </dl>
        </Tile>
        <Tile id="public-pay" title={t("payments.online")} span="col-span-5">
          <div role="status" aria-live="polite" aria-atomic="true">
            {payment.isPending ? <p>{t("common.loading")}</p> : null}
            {review ? (
              <Notice
                live={false}
                variant="warning"
                title={t("payments.review")}
              >
                {t("payments.reviewHint")}
              </Notice>
            ) : current ? (
              <p className="font-semibold">
                {t(`payments.status.${paymentStatusKey(current.status)}`)}
              </p>
            ) : null}
            {!payable ? (
              <Notice
                live={false}
                variant="success"
                title={t("payments.nothingDue")}
              />
            ) : null}
            {payable &&
            (setupRequired || inv.online_payment_enabled === false) &&
            payment.isSuccess &&
            !resumable &&
            !review ? (
              <Notice
                live={false}
                variant="warning"
                title={t("payments.onlineUnavailable")}
              >
                {t("payments.onlineUnavailableHint")}
              </Notice>
            ) : null}
          </div>
          {payment.isError ? (
            <LoadError
              error={payment.error}
              onRetry={() => void payment.refetch()}
            />
          ) : null}
          {resumable && checkout ? (
            <>
              <a
                className={buttonVariants({ size: "lg" })}
                href={checkout}
                rel="noreferrer"
              >
                {t("payments.continue")}
              </a>
              {/* Same-origin proxy keeps provider credentials and arbitrary image hosts out of the browser. */}
              <img
                src={`/v1${path}/payment/qr`}
                alt={t("payments.qr")}
                width={240}
                height={240}
                className="mx-auto max-w-full rounded-xl"
              />
            </>
          ) : null}
          {payable &&
          !setupRequired &&
          inv.online_payment_enabled === true &&
          !review &&
          (!current ||
            [
              "paid",
              "refunded",
              "failed",
              "cancelled",
              "canceled",
              "expired",
            ].includes(current.status)) &&
          payment.isSuccess ? (
            <form
              className="flex flex-col gap-4"
              onSubmit={(event) => {
                event.preventDefault()
                const form = new FormData(event.currentTarget)
                const email = String(form.get("payer_email") ?? "").trim()
                create.mutate({
                  country: String(form.get("country")).trim().toUpperCase(),
                  payer_name: String(form.get("payer_name")).trim(),
                  ...(email ? { payer_email: email } : {}),
                })
              }}
            >
              <Field
                id="payer-name"
                label={t("payments.payerName")}
                errors={errors.payer_name}
              >
                <Input
                  name="payer_name"
                  autoComplete="name"
                  maxLength={255}
                  required
                />
              </Field>
              <Field
                id="payer-country"
                label={t("payments.country")}
                errors={errors.country}
              >
                <Input
                  name="country"
                  autoComplete="country"
                  minLength={2}
                  maxLength={2}
                  pattern="[A-Za-z]{2}"
                  placeholder="NL"
                  required
                />
              </Field>
              <Field
                id="payer-email"
                label={t("payments.payerEmail")}
                errors={errors.payer_email}
              >
                <Input
                  name="payer_email"
                  type="email"
                  autoComplete="email"
                  maxLength={255}
                />
              </Field>
              <ApiError error={create.error} fallback={t("common.error")} />
              <Button type="submit" size="lg" disabled={create.isPending}>
                {t("payments.pay")}
              </Button>
            </form>
          ) : null}
          {current || review ? (
            <Button
              variant="outline"
              disabled={payment.isFetching}
              onClick={() => void payment.refetch()}
            >
              {t("payments.refresh")}
            </Button>
          ) : null}
        </Tile>
        <Tile id="public-bank" title={t("payments.bank")} span="col-span-12">
          <dl className="grid gap-3 text-sm md:grid-cols-3">
            {(
              [
                [
                  "accountName",
                  account.account_name ?? account.name ?? company.account_name,
                ],
                [
                  "accountNumber",
                  account.account_number ?? company.account_number,
                ],
                ["bic", account.bic_swift ?? company.bic_swift],
              ] as const
            ).map(([key, value]) =>
              text(value) ? (
                <div key={key}>
                  <dt className="text-muted-foreground">
                    {t(`payments.${key}`)}
                  </dt>
                  <dd className="font-medium break-all">{text(value)}</dd>
                </div>
              ) : null
            )}
          </dl>
          <p className="text-sm text-muted-foreground">
            {t("payments.bankReference", {
              reference: inv.invoice_nr ?? inv.reference ?? "—",
            })}
          </p>
        </Tile>
      </Bento>
    </main>
  )
}
