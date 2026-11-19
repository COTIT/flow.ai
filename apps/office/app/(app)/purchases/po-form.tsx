"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { Trash2 } from "lucide-react"

import { useSession } from "@verzo/api-client/auth"
import { useCurrenciesList } from "@verzo/api-client/endpoints/currencies"
import { useProductsList } from "@verzo/api-client/endpoints/products"
import {
  usePurchaseOrdersCreate,
  usePurchaseOrdersUpdate,
} from "@verzo/api-client/endpoints/purchase-orders"
import { usePurchaseUnitsList } from "@verzo/api-client/endpoints/purchase-units"
import { useRelationsList } from "@verzo/api-client/endpoints/relations"
import { useWarehousesList } from "@verzo/api-client/endpoints/warehouses"
import type {
  OrderPayload,
  PurchaseOrderResource,
} from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { Card } from "@verzo/ui/components/card"
import { Input } from "@verzo/ui/components/input"
import { Textarea } from "@verzo/ui/components/textarea"
import { Field, selectClass } from "@/components/field"
import { Capped } from "@/components/states"
import { useIdempotencyKey } from "@/lib/idempotency"

import { isStale, today, useReasons, useRefreshOrder } from "./shared"

type Line = {
  key: string
  product_id: string
  quantity: string
  purchase_unit_id: string
  unit_price: string
  warehouse_id: string
}

const blankLine = (): Line => ({
  key: crypto.randomUUID(),
  product_id: "",
  quantity: "",
  purchase_unit_id: "",
  unit_price: "",
  warehouse_id: "",
})

/**
 * The draft order form (Form card of OfficeProduct): create, or full replace of a draft.
 * Amounts go to the API as typed; line totals and the order total come back from it.
 */
export function PoForm({ order }: { order?: PurchaseOrderResource }) {
  const t = useTranslations()
  const router = useRouter()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const suppliers = useRelationsList(workspaceId, { suppliers: true, limit: 100 }, enabled)
  const currencies = useCurrenciesList(workspaceId, { page_size: 100 }, enabled)
  const products = useProductsList(workspaceId, { page_size: 100 }, enabled)
  const warehouses = useWarehousesList(workspaceId, { page_size: 100 }, enabled)
  const refresh = useRefreshOrder(workspaceId, order?.id ?? "")
  const key = useIdempotencyKey()
  const [lines, setLines] = React.useState<Line[]>(() =>
    order?.lines.length
      ? order.lines.map((l) => ({
          key: l.id,
          product_id: l.product.id,
          quantity: l.input_quantity,
          purchase_unit_id: l.purchase_unit?.id ?? "",
          unit_price: l.unit_price,
          warehouse_id: l.warehouse_id ?? "",
        }))
      : [blankLine()]
  )
  const done = (id: string, message: "created" | "saved") => {
    refresh()
    router.push(`/purchases/${id}?done=${message}`)
  }
  const create = usePurchaseOrdersCreate({
    mutation: {
      onSuccess: (r) => done(r.data.id, "created"),
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const update = usePurchaseOrdersUpdate({
    mutation: {
      onSuccess: (r) => done(r.data.id, "saved"),
      onError: (e) => {
        if (isStale(e)) refresh()
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const mutation = order ? update : create
  const errors = fieldErrors(mutation.error)
  const err = (field: string) => errors[`payload.${field}`]
  const setLine = (k: string, patch: Partial<Line>) =>
    setLines((ls) => ls.map((l) => (l.key === k ? { ...l, ...patch } : l)))
  const defaultCurrency = currencies.data?.data.find((c) => c.default_currency)?.id

  return (
    <form
      className="flex flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault()
        const f = new FormData(e.currentTarget)
        const text = (k: string) => String(f.get(k) ?? "").trim()
        const payload: OrderPayload = {
          supplier_id: text("supplier_id"),
          currency_id: text("currency_id"),
          purchase_date: text("purchase_date"),
          expected_date: text("expected_date") || null,
          reference: text("reference") || null,
          note: text("note") || null,
          default_warehouse_id: text("default_warehouse_id") || null,
          lines: lines.map((l) => ({
            product_id: l.product_id,
            quantity: l.quantity.trim(),
            unit_price: l.unit_price.trim(),
            purchase_unit_id: l.purchase_unit_id || null,
            warehouse_id: l.warehouse_id || null,
          })),
        }
        const client_ref = key.request.headers["Idempotency-Key"]
        if (order)
          update.mutate({
            workspace: workspaceId,
            order: order.id,
            data: { client_ref, expected_version: order.version, payload },
          })
        else create.mutate({ workspace: workspaceId, data: { client_ref, payload } })
      }}
    >
      <Card className="gap-4 px-6 py-6 max-md:p-4">
        <Capped meta={suppliers.data?.meta} />
        <div className="grid gap-4 md:grid-cols-2">
          <Field id="po-supplier" label={t("purchases.supplier")} errors={err("supplier_id")}>
            <select
              name="supplier_id"
              className={selectClass}
              required
              defaultValue={order?.supplier.id ?? ""}
            >
              <option value="" disabled>
                {t("purchases.chooseSupplier")}
              </option>
              {suppliers.data?.data.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.company_name ?? s.alias ?? s.relation_nr ?? s.id}
                </option>
              ))}
            </select>
          </Field>
          <Field id="po-currency" label={t("purchases.currency")} errors={err("currency_id")}>
            <select
              // Remount once the list arrives so the workspace default can preselect.
              key={defaultCurrency ?? "none"}
              name="currency_id"
              className={selectClass}
              required
              defaultValue={order?.currency.id ?? defaultCurrency ?? ""}
            >
              <option value="" disabled>
                {t("purchases.chooseCurrency")}
              </option>
              {currencies.data?.data.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.asset_type}
                </option>
              ))}
            </select>
          </Field>
          <Field id="po-date" label={t("purchases.purchaseDate")} errors={err("purchase_date")}>
            <Input
              type="date"
              name="purchase_date"
              required
              defaultValue={order?.purchase_date ?? today()}
            />
          </Field>
          <Field id="po-expected" label={t("purchases.expectedDate")} errors={err("expected_date")}>
            <Input type="date" name="expected_date" defaultValue={order?.expected_date ?? ""} />
          </Field>
          <Field id="po-reference" label={t("purchases.reference")} errors={err("reference")}>
            <Input name="reference" maxLength={255} defaultValue={order?.reference ?? ""} />
          </Field>
          <Field
            id="po-warehouse"
            label={t("purchases.defaultWarehouse")}
            errors={err("default_warehouse_id")}
          >
            <select
              name="default_warehouse_id"
              className={selectClass}
              defaultValue={order?.default_warehouse_id ?? ""}
            >
              <option value="">{t("purchases.noWarehouse")}</option>
              {warehouses.data?.data.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <Field id="po-note" label={t("purchases.note")} errors={err("note")}>
          <Textarea name="note" maxLength={1000} defaultValue={order?.note ?? ""} />
        </Field>
      </Card>

      <Card className="gap-4 px-6 py-6 max-md:p-4" role="region" aria-labelledby="po-lines">
        <h2
          id="po-lines"
          className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
        >
          {t("purchases.lines")}
        </h2>
        <Capped meta={products.data?.meta} />
        {err("lines")?.length ? (
          <p className="text-sm text-destructive">{err("lines")!.join(" ")}</p>
        ) : null}
        {lines.map((l, i) => (
          <fieldset
            key={l.key}
            className="grid gap-3 border-t border-(--hair) pt-4 first-of-type:border-0 first-of-type:pt-0 md:grid-cols-[2fr_1fr_1fr_1fr_1.5fr_auto] md:items-end"
          >
            <legend className="sr-only">{t("purchases.line", { n: i + 1 })}</legend>
            <Field
              id={`po-${l.key}-product`}
              label={t("purchases.product")}
              errors={err(`lines.${i}.product_id`) ?? err(`lines.${i}`)}
            >
              <select
                className={selectClass}
                required
                value={l.product_id}
                onChange={(e) =>
                  setLine(l.key, { product_id: e.target.value, purchase_unit_id: "" })
                }
              >
                <option value="" disabled>
                  {t("purchases.chooseProduct")}
                </option>
                {products.data?.data.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field
              id={`po-${l.key}-qty`}
              label={t("purchases.quantity")}
              errors={err(`lines.${i}.quantity`) ?? err(`lines.${i}.base_quantity`)}
            >
              <Input
                inputMode="decimal"
                required
                value={l.quantity}
                onChange={(e) => setLine(l.key, { quantity: e.target.value })}
              />
            </Field>
            <UnitSelect
              id={`po-${l.key}-unit`}
              workspaceId={workspaceId}
              product={l.product_id}
              value={l.purchase_unit_id}
              onChange={(v) => setLine(l.key, { purchase_unit_id: v })}
              errors={err(`lines.${i}.purchase_unit_id`)}
            />
            <Field
              id={`po-${l.key}-price`}
              label={t("purchases.unitPrice")}
              errors={err(`lines.${i}.unit_price`)}
            >
              <Input
                inputMode="decimal"
                required
                value={l.unit_price}
                onChange={(e) => setLine(l.key, { unit_price: e.target.value })}
              />
            </Field>
            <Field
              id={`po-${l.key}-wh`}
              label={t("purchases.warehouse")}
              errors={err(`lines.${i}.warehouse_id`)}
            >
              <select
                className={selectClass}
                value={l.warehouse_id}
                onChange={(e) => setLine(l.key, { warehouse_id: e.target.value })}
              >
                <option value="">{t("purchases.defaultWarehouse")}</option>
                {warehouses.data?.data.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))}
              </select>
            </Field>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="max-md:justify-self-end"
              aria-label={t("purchases.removeLine", { n: i + 1 })}
              disabled={lines.length === 1}
              onClick={() => setLines((ls) => ls.filter((x) => x.key !== l.key))}
            >
              <Trash2 className="size-4" />
            </Button>
          </fieldset>
        ))}
        <Button
          type="button"
          variant="outline"
          className="self-start"
          onClick={() => setLines((ls) => [...ls, blankLine()])}
        >
          {t("purchases.addLine")}
        </Button>
      </Card>

      <ApiError error={mutation.error} fallback={t("common.error")} codes={reasons} />
      <Button type="submit" size="lg" className="self-end max-md:w-full" disabled={mutation.isPending}>
        {t("purchases.saveDraft")}
      </Button>
    </form>
  )
}

/** The product's purchase units (box of 12…); "single items" sends none. */
function UnitSelect({
  id,
  workspaceId,
  product,
  value,
  onChange,
  errors,
}: {
  id: string
  workspaceId: string
  product: string
  value: string
  onChange: (v: string) => void
  errors?: string[]
}) {
  const t = useTranslations("purchases")
  const units = usePurchaseUnitsList(workspaceId, product, {
    query: { enabled: Boolean(workspaceId && product) },
  })
  return (
    <Field id={id} label={t("unit")} errors={errors}>
      <select className={selectClass} value={value} onChange={(e) => onChange(e.target.value)}>
        <option value="">{t("baseUnit")}</option>
        {units.data?.data
          .filter((u) => u.active || u.id === value)
          .map((u) => (
            <option key={u.id} value={u.id}>
              {u.code}
            </option>
          ))}
      </select>
    </Field>
  )
}
