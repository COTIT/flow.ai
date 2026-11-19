"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@verzo/api-client/auth"
import {
  useAccountingBankAccountsList,
  useAccountingCashPlacesList,
} from "@verzo/api-client/endpoints/accounting"
import { useCurrenciesList } from "@verzo/api-client/endpoints/currencies"
import { useRelationsList } from "@verzo/api-client/endpoints/relations"
import {
  getSupplierPaymentsListQueryKey,
  supplierPaymentsCreate,
  supplierPaymentsCreateReview,
} from "@verzo/api-client/endpoints/supplier-payments"
import type { PaymentPayload, PaymentReview } from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { Card } from "@verzo/ui/components/card"
import { Input } from "@verzo/ui/components/input"
import { Textarea } from "@verzo/ui/components/textarea"
import { formatDay } from "@verzo/ui/lib/format"
import { Field, selectClass } from "@/components/field"
import { PageHead } from "@/components/list"
import { Fact, JournalList, usePayReasons, usePayWord } from "@/components/payables"
import { Capped, NoAccess } from "@/components/states"
import { useCan, useRights } from "@/lib/can"
import { useIdempotencyKey } from "@/lib/idempotency"

import { showMoney, today } from "../../purchases/shared"

type Form = {
  supplier_id: string
  currency_id: string
  direction: "payment" | "refund_received"
  amount: string
  date: string
  place: string
  reference: string
  evidence_ref: string
  confirmation_method: string
  note: string
}

/**
 * Record money paid to (or refunded by) a supplier: fill, read the server's review (rate,
 * functional amount, journal), then record with its fingerprint. Editing drops the review.
 */
export default function NewSupplierPaymentPage({
  searchParams,
}: {
  searchParams: Promise<{ direction?: string; supplier?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const word = usePayWord()
  const reasons = usePayReasons()
  const queryClient = useQueryClient()
  const rights = useRights()
  const may = useCan("supplierpayment.create")
  const q = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  // shortcut: first 100 of each picker list; page when a workspace holds more.
  const suppliers = useRelationsList(workspaceId, { suppliers: true, limit: 100 }, enabled)
  const currencies = useCurrenciesList(workspaceId, { page_size: 100 }, enabled)
  const places = useAccountingCashPlacesList(workspaceId, enabled)
  const banks = useAccountingBankAccountsList(workspaceId, enabled)
  const key = useIdempotencyKey()
  const [f, setF] = React.useState<Form>({
    supplier_id: q.supplier ?? "",
    currency_id: "",
    direction: q.direction === "refund_received" ? "refund_received" : "payment",
    amount: "",
    date: today(),
    place: "",
    reference: "",
    evidence_ref: "",
    confirmation_method: "bank_statement",
    note: "",
  })
  const [review, setReview] = React.useState<PaymentReview | null>(null)
  const [error, setError] = React.useState<unknown>(null)
  const [busy, setBusy] = React.useState(false)
  const set = (patch: Partial<Form>) => {
    setF((x) => ({ ...x, ...patch }))
    setReview(null)
  }
  const defaultCurrency = currencies.data?.data.find((c) => c.default_currency)?.id ?? ""
  const currency = f.currency_id || defaultCurrency
  // Display only: the API refuses a place of another currency or a drawer without a shift.
  const placeOptions = [
    ...(places.data?.data ?? [])
      .filter((p) => p.currency.id === currency)
      .map((p) => ({ value: `cash:${p.kind}:${p.id}`, label: `${p.name} (${word("placeKinds", p.kind)})` })),
    ...(banks.data?.data ?? [])
      .filter((b) => b.active && b.currency_id === currency)
      .map((b) => ({ value: `bank:${b.id}`, label: b.name })),
  ]
  const isBank = f.place.startsWith("bank:")
  const payload = (): PaymentPayload => {
    const [kind, a, b] = f.place.split(":")
    return {
      supplier_id: f.supplier_id,
      currency_id: currency,
      direction: f.direction,
      amount: f.amount.trim(),
      date: f.date,
      reference: f.reference.trim() || null,
      evidence_ref: f.evidence_ref.trim() || null,
      note: f.note.trim() || null,
      location:
        kind === "bank"
          ? {
              kind: "bank_account",
              bank_account_id: a,
              reference: f.reference.trim() || null,
              evidence_ref: f.evidence_ref.trim() || null,
              confirmation_method: f.confirmation_method,
            }
          : kind === "cash"
            ? { kind: "cash_place", cash_place: { kind: a, id: b ?? "" } }
            : null,
    }
  }
  const errors = fieldErrors(error)
  const err = (k: string) => errors[k] ?? errors[`payload.${k}`]
  if (rights.read && !may) return <NoAccess />

  const run = async (record: boolean) => {
    setBusy(true)
    setError(null)
    try {
      if (!record) {
        setReview((await supplierPaymentsCreateReview(workspaceId, payload())).data)
      } else if (review) {
        const r = await supplierPaymentsCreate(
          workspaceId,
          {
            client_ref: key.request.headers["Idempotency-Key"],
            payload: { ...payload(), review_fingerprint: review.review_fingerprint },
          },
          key.request
        )
        key.settle(null)
        void queryClient.invalidateQueries({ queryKey: getSupplierPaymentsListQueryKey(workspaceId) })
        router.push(`/supplier-payments/${r.data.id}?done=created`)
      }
    } catch (e) {
      if (record) key.settle(e)
      setError(e)
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <PageHead
        back={{ href: "/supplier-payments", label: t("payables.payments.title") }}
        eyebrow={t("shell.buy")}
        title={t("payables.payments.newTitle")}
      />
      <form
        className="flex flex-col gap-5"
        onSubmit={(e) => {
          e.preventDefault()
          void run(Boolean(review))
        }}
      >
        <Card className="gap-4 px-6 py-6 max-md:p-4">
          <Capped meta={suppliers.data?.meta} />
          <fieldset className="grid gap-4 md:grid-cols-2" disabled={busy}>
            <Field id="pay-direction" label={t("payables.payments.direction")} errors={err("direction")}>
              <select
                className={selectClass}
                value={f.direction}
                onChange={(e) => set({ direction: e.target.value as Form["direction"] })}
              >
                <option value="payment">{word("directions", "payment")}</option>
                <option value="refund_received">{word("directions", "refund_received")}</option>
              </select>
            </Field>
            <Field id="pay-supplier" label={t("payables.supplier")} errors={err("supplier_id")}>
              <select
                className={selectClass}
                required
                value={f.supplier_id}
                onChange={(e) => set({ supplier_id: e.target.value })}
              >
                <option value="" disabled>
                  {t("payables.chooseSupplier")}
                </option>
                {suppliers.data?.data.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.company_name ?? s.alias ?? s.relation_nr ?? s.id}
                  </option>
                ))}
              </select>
            </Field>
            <Field id="pay-currency" label={t("payables.currency")} errors={err("currency_id")}>
              <select
                className={selectClass}
                required
                value={currency}
                onChange={(e) => set({ currency_id: e.target.value, place: "" })}
              >
                <option value="" disabled>
                  {t("payables.chooseCurrency")}
                </option>
                {currencies.data?.data.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.asset_type}
                  </option>
                ))}
              </select>
            </Field>
            <Field id="pay-amount" label={t("payables.amount")} errors={err("amount")}>
              <Input inputMode="decimal" required value={f.amount} onChange={(e) => set({ amount: e.target.value })} />
            </Field>
            <Field id="pay-date" label={t("payables.payments.date")} errors={err("date")}>
              <Input type="date" required value={f.date} onChange={(e) => set({ date: e.target.value })} />
            </Field>
            <Field
              id="pay-place"
              label={t("payables.payments.place")}
              errors={err("location") ?? err("location.kind") ?? err("location.cash_place")}
            >
              <select className={selectClass} required value={f.place} onChange={(e) => set({ place: e.target.value })}>
                <option value="" disabled>
                  {t("payables.payments.choosePlace")}
                </option>
                {placeOptions.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </Field>
            <Field
              id="pay-reference"
              label={t("payables.payments.reference")}
              errors={err("reference") ?? err("location.reference")}
            >
              <Input
                maxLength={255}
                required={isBank}
                value={f.reference}
                onChange={(e) => set({ reference: e.target.value })}
              />
            </Field>
            <Field
              id="pay-evidence"
              label={t("payables.evidenceRef")}
              errors={err("evidence_ref") ?? err("location.evidence_ref")}
            >
              <Input
                maxLength={255}
                required
                value={f.evidence_ref}
                onChange={(e) => set({ evidence_ref: e.target.value })}
              />
            </Field>
            {isBank ? (
              <Field
                id="pay-confirmation"
                label={t("payables.payments.confirmation")}
                errors={err("location.confirmation_method")}
              >
                <select
                  className={selectClass}
                  value={f.confirmation_method}
                  onChange={(e) => set({ confirmation_method: e.target.value })}
                >
                  <option value="bank_statement">{word("confirmations", "bank_statement")}</option>
                  <option value="provider_dashboard">{word("confirmations", "provider_dashboard")}</option>
                </select>
              </Field>
            ) : null}
          </fieldset>
          <Field id="pay-note" label={t("payables.note")} errors={err("note")}>
            <Textarea maxLength={1000} value={f.note} onChange={(e) => set({ note: e.target.value })} />
          </Field>
        </Card>

        {review ? (
          <Card role="region" aria-labelledby="pay-review" className="gap-2 px-6 py-6 text-[13px] max-md:p-4">
            <h2 id="pay-review" className="text-sm font-semibold">
              {t("payables.reviewTitle")}
            </h2>
            <Fact label={t("payables.payments.direction")} value={word("directions", review.direction)} />
            <Fact label={t("payables.payments.date")} value={formatDay(review.date, locale)} />
            <Fact label={t("payables.amount")} value={showMoney(review.amount, review.currency, locale)} />
            {review.functional_amount ? (
              <Fact
                label={t("payables.functionalTotal")}
                value={showMoney(review.functional_amount, review.functional_currency, locale)}
              />
            ) : null}
            <JournalList journal={review.journal} />
          </Card>
        ) : null}

        <ApiError error={error} fallback={t("common.error")} codes={reasons} />
        <div className="flex justify-end gap-2 max-md:flex-col">
          {review ? (
            <Button type="button" variant="outline" size="lg" onClick={() => setReview(null)}>
              {t("payables.change")}
            </Button>
          ) : null}
          <Button type="submit" size="lg" disabled={busy}>
            {review ? t("payables.payments.record") : t("payables.review")}
          </Button>
        </div>
      </form>
    </>
  )
}
