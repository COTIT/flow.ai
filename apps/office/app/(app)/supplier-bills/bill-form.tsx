"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"
import { Trash2 } from "lucide-react"

import { useSession } from "@verzo/api-client/auth"
import { useCurrenciesList } from "@verzo/api-client/endpoints/currencies"
import { useProductsList } from "@verzo/api-client/endpoints/products"
import { useRelationsList } from "@verzo/api-client/endpoints/relations"
import {
  getSupplierBillsGetQueryKey,
  getSupplierBillsListQueryKey,
  useSupplierBillsCreate,
  useSupplierBillsUpdate,
} from "@verzo/api-client/endpoints/supplier-bills"
import { useTaxProfilesList } from "@verzo/api-client/endpoints/tax-profiles"
import type {
  AccountResource,
  BillPayload,
  SupplierBillResource,
} from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { Card } from "@verzo/ui/components/card"
import { Input } from "@verzo/ui/components/input"
import { Textarea } from "@verzo/ui/components/textarea"
import { Field, selectClass } from "@/components/field"
import { usePayReasons } from "@/components/payables"
import { AccountPicker } from "@/components/pickers"
import { Capped } from "@/components/states"
import { useIdempotencyKey } from "@/lib/idempotency"

import { isStale, today } from "../purchases/shared"

type Line = {
  key: string
  kind: "goods" | "expense"
  target: string
  quantity: string
  unit_price: string
  tax_profile_id: string
  description: string
}

const blankLine = (): Line => ({
  key: crypto.randomUUID(),
  kind: "goods",
  target: "",
  quantity: "",
  unit_price: "",
  tax_profile_id: "",
  description: "",
})

/**
 * The draft bill form (Form card of the purchase order form): create, or full replace of a
 * draft. Each line is goods (a product, matched to receipts later) or an expense account.
 * Extensions and totals come back from the API.
 */
export function BillForm({ bill }: { bill?: SupplierBillResource }) {
  const t = useTranslations()
  const router = useRouter()
  const queryClient = useQueryClient()
  const reasons = usePayReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  // shortcut: first 100 of each picker list; page when a workspace holds more.
  const suppliers = useRelationsList(workspaceId, { suppliers: true, limit: 100 }, enabled)
  const currencies = useCurrenciesList(workspaceId, { page_size: 100 }, enabled)
  const products = useProductsList(workspaceId, { page_size: 100 }, enabled)
  const taxes = useTaxProfilesList(workspaceId, { page_size: 100 }, enabled)
  const key = useIdempotencyKey()
  const [lines, setLines] = React.useState<Line[]>(() =>
    bill?.lines.length
      ? bill.lines
          .filter((l) => l.kind !== "landed_cost")
          .map((l) => ({
            key: l.id,
            kind: l.kind === "expense" ? "expense" : "goods",
            target: l.product_id ?? l.expense_account_id ?? "",
            quantity: l.quantity,
            unit_price: l.unit_price,
            tax_profile_id: l.tax_profile_id ?? "",
            description: l.description ?? "",
          }))
      : [blankLine()]
  )
  const refresh = (id: string) => {
    void queryClient.invalidateQueries({ queryKey: getSupplierBillsListQueryKey(workspaceId) })
    void queryClient.invalidateQueries({ queryKey: getSupplierBillsGetQueryKey(workspaceId, id) })
  }
  const done = (id: string, message: "created" | "saved") => {
    refresh(id)
    router.push(`/supplier-bills/${id}?done=${message}`)
  }
  const create = useSupplierBillsCreate({
    mutation: {
      onSuccess: (r) => done(r.data.id, "created"),
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const update = useSupplierBillsUpdate({
    mutation: {
      onSuccess: (r) => done(r.data.id, "saved"),
      onError: (e) => {
        if (isStale(e) && bill) refresh(bill.id)
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const mutation = bill ? update : create
  const errors = fieldErrors(mutation.error)
  const err = (field: string) => errors[`payload.${field}`]
  const setLine = (k: string, patch: Partial<Line>) =>
    setLines((ls) => ls.map((l) => (l.key === k ? { ...l, ...patch } : l)))
  const defaultCurrency = currencies.data?.data.find((c) => c.default_currency)?.id
  // Display only: the API refuses a mapped control or inactive account.
  const expenseAccount = (a: AccountResource) =>
    a.type === "expense" && a.active && a.roles.length === 0
  const taxLabel = (p: { components: { name: string }[]; country: string }) =>
    `${p.components.map((c) => c.name).join(" + ")} (${p.country})`

  return (
    <form
      className="flex flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault()
        const f = new FormData(e.currentTarget)
        const text = (k: string) => String(f.get(k) ?? "").trim()
        const payload: BillPayload = {
          supplier_id: text("supplier_id"),
          currency_id: text("currency_id"),
          supplier_invoice_number: text("supplier_invoice_number"),
          invoice_date: text("invoice_date"),
          due_date: text("due_date") || null,
          accounting_date: text("accounting_date"),
          evidence_ref: text("evidence_ref") || null,
          note: text("note") || null,
          corrects_bill_id: bill?.corrects_bill_id ?? null,
          lines: lines.map((l) => ({
            product_id: l.kind === "goods" ? l.target : null,
            expense_account_id: l.kind === "expense" ? l.target : null,
            quantity: l.quantity.trim(),
            unit_price: l.unit_price.trim(),
            tax_profile_id: l.tax_profile_id || null,
            description: l.description.trim() || null,
          })),
        }
        const client_ref = key.request.headers["Idempotency-Key"]
        if (bill)
          update.mutate({
            workspace: workspaceId,
            bill: bill.id,
            data: { client_ref, expected_version: bill.version, payload },
          })
        else create.mutate({ workspace: workspaceId, data: { client_ref, payload } })
      }}
    >
      <Card className="gap-4 px-6 py-6 max-md:p-4">
        <Capped meta={suppliers.data?.meta} />
        <div className="grid gap-4 md:grid-cols-2">
          <Field id="bill-supplier" label={t("payables.supplier")} errors={err("supplier_id")}>
            <select name="supplier_id" className={selectClass} required defaultValue={bill?.supplier.id ?? ""}>
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
          <Field id="bill-currency" label={t("payables.currency")} errors={err("currency_id")}>
            <select
              key={defaultCurrency ?? "none"}
              name="currency_id"
              className={selectClass}
              required
              defaultValue={bill?.currency.id ?? defaultCurrency ?? ""}
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
          <Field
            id="bill-invoice"
            label={t("payables.invoiceNumber")}
            errors={err("supplier_invoice_number")}
          >
            <Input
              name="supplier_invoice_number"
              required
              maxLength={255}
              defaultValue={bill?.supplier_invoice_number ?? ""}
            />
          </Field>
          <Field id="bill-evidence" label={t("payables.evidenceRef")} errors={err("evidence_ref")}>
            <Input name="evidence_ref" maxLength={255} defaultValue={bill?.evidence_ref ?? ""} />
          </Field>
          <Field id="bill-invoice-date" label={t("payables.invoiceDate")} errors={err("invoice_date")}>
            <Input type="date" name="invoice_date" required defaultValue={bill?.invoice_date ?? today()} />
          </Field>
          <Field id="bill-due" label={t("payables.dueDate")} errors={err("due_date")}>
            <Input type="date" name="due_date" defaultValue={bill?.due_date ?? ""} />
          </Field>
          <Field
            id="bill-accounting-date"
            label={t("payables.accountingDate")}
            errors={err("accounting_date")}
          >
            <Input
              type="date"
              name="accounting_date"
              required
              defaultValue={bill?.accounting_date ?? today()}
            />
          </Field>
        </div>
        <Field id="bill-note" label={t("payables.note")} errors={err("note")}>
          <Textarea name="note" maxLength={1000} defaultValue={bill?.note ?? ""} />
        </Field>
      </Card>

      <Card className="gap-4 px-6 py-6 max-md:p-4" role="region" aria-labelledby="bill-lines">
        <h2 id="bill-lines" className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase">
          {t("payables.lines")}
        </h2>
        {err("lines")?.length ? <p className="text-sm text-destructive">{err("lines")!.join(" ")}</p> : null}
        {lines.map((l, i) => {
          const le = (f: string) => err(`lines.${i}.${f}`)
          return (
            <fieldset
              key={l.key}
              className="grid gap-3 border-t border-(--hair) pt-4 first-of-type:border-0 first-of-type:pt-0 md:grid-cols-[1fr_2fr_1fr_1fr_1.5fr_auto] md:items-end"
            >
              <legend className="sr-only">{t("payables.line", { n: i + 1 })}</legend>
              <Field id={`bl-${l.key}-kind`} label={t("payables.lineKind")}>
                <select
                  className={selectClass}
                  value={l.kind}
                  onChange={(e) => setLine(l.key, { kind: e.target.value as Line["kind"], target: "" })}
                >
                  <option value="goods">{t("payables.kinds.goods")}</option>
                  <option value="expense">{t("payables.kinds.expense")}</option>
                </select>
              </Field>
              <Field
                id={`bl-${l.key}-target`}
                label={l.kind === "goods" ? t("payables.product") : t("payables.expenseAccount")}
                errors={le("product_id") ?? le("expense_account_id") ?? err(`lines.${i}`)}
              >
                {l.kind === "goods" ? (
                  <select
                    className={selectClass}
                    required
                    value={l.target}
                    onChange={(e) => setLine(l.key, { target: e.target.value })}
                  >
                    <option value="" disabled>
                      {t("payables.chooseProduct")}
                    </option>
                    {products.data?.data.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                  </select>
                ) : (
                  <AccountPicker
                    value={l.target}
                    onChange={(id) => setLine(l.key, { target: id })}
                    filter={expenseAccount}
                    placeholder={t("payables.chooseAccount")}
                  />
                )}
              </Field>
              <Field id={`bl-${l.key}-qty`} label={t("payables.quantity")} errors={le("quantity")}>
                <Input
                  inputMode="decimal"
                  required
                  value={l.quantity}
                  onChange={(e) => setLine(l.key, { quantity: e.target.value })}
                />
              </Field>
              <Field id={`bl-${l.key}-price`} label={t("payables.unitPrice")} errors={le("unit_price")}>
                <Input
                  inputMode="decimal"
                  required
                  value={l.unit_price}
                  onChange={(e) => setLine(l.key, { unit_price: e.target.value })}
                />
              </Field>
              <Field id={`bl-${l.key}-tax`} label={t("payables.tax")} errors={le("tax_profile_id")}>
                <select
                  className={selectClass}
                  value={l.tax_profile_id}
                  onChange={(e) => setLine(l.key, { tax_profile_id: e.target.value })}
                >
                  <option value="">{t("payables.noTax")}</option>
                  {taxes.data?.data.map((p) => (
                    <option key={p.id} value={p.id}>
                      {taxLabel(p)}
                    </option>
                  ))}
                </select>
              </Field>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="max-md:justify-self-end"
                aria-label={t("payables.removeLine", { n: i + 1 })}
                disabled={lines.length === 1}
                onClick={() => setLines((ls) => ls.filter((x) => x.key !== l.key))}
              >
                <Trash2 className="size-4" />
              </Button>
              <div className="md:col-span-6">
                <Field id={`bl-${l.key}-desc`} label={t("payables.description")} errors={le("description")}>
                  <Input
                    maxLength={255}
                    value={l.description}
                    onChange={(e) => setLine(l.key, { description: e.target.value })}
                  />
                </Field>
              </div>
            </fieldset>
          )
        })}
        <Button
          type="button"
          variant="outline"
          className="self-start"
          onClick={() => setLines((ls) => [...ls, blankLine()])}
        >
          {t("payables.addLine")}
        </Button>
      </Card>

      <ApiError error={mutation.error} fallback={t("common.error")} codes={reasons} />
      <Button type="submit" size="lg" className="self-end max-md:w-full" disabled={mutation.isPending}>
        {t("payables.saveDraft")}
      </Button>
    </form>
  )
}
