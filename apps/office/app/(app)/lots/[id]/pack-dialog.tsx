"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import { getLotsListQueryKey } from "@genesy/api-client/endpoints/lots"
import { useProductsList } from "@genesy/api-client/endpoints/products"
import { useSupplyChainPack } from "@genesy/api-client/endpoints/supply-chain"
import { useWarehousesList } from "@genesy/api-client/endpoints/warehouses"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Field, selectClass } from "@/components/field"
import { Capped } from "@/components/states"

/**
 * Pack this lot's kg into a new lot of jars. Offered on every lot: the API says which lots
 * can be packed (only supply lots, once per lineage) and whether the kg are free.
 */
export function PackDialog({ lotId, code }: { lotId: string; code: string }) {
  const t = useTranslations()
  const queryClient = useQueryClient()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const enabled = { query: { enabled: open && Boolean(workspaceId) } }
  // shortcut: first 100 products and warehouses in the pickers; add search when catalogs grow.
  const products = useProductsList(workspaceId, { page_size: 100 }, enabled)
  const warehouses = useWarehousesList(workspaceId, { page_size: 100 }, enabled)
  const pack = useSupplyChainPack({
    mutation: {
      onSuccess: () =>
        queryClient.invalidateQueries({
          queryKey: getLotsListQueryKey(workspaceId),
        }),
    },
  })
  const errors = fieldErrors(pack.error)
  const packed = pack.data?.data

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) pack.reset()
      }}
    >
      <DialogTrigger render={<Button variant="outline" />}>
        {t("packing.pack")}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("packing.title", { code })}</DialogTitle>
          <DialogDescription>{t("packing.text")}</DialogDescription>
        </DialogHeader>
        {packed ? (
          <div role="status" className="flex flex-col gap-2 text-sm">
            {/* The two exact numbers as the API keeps them; no kg-per-jar division here. */}
            <p>
              {t("packing.done", {
                code: packed.code,
                units: packed.pack_units,
                kg: packed.pack_raw_kg,
              })}
            </p>
            <Link
              className="font-medium underline-offset-4 hover:underline"
              href={`/lots/${packed.id}`}
            >
              {t("packing.open", { code: packed.code })}
            </Link>
          </div>
        ) : (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              const f = new FormData(e.currentTarget)
              const text = (k: string) => String(f.get(k) ?? "").trim()
              pack.mutate({
                workspace: workspaceId,
                lot: lotId,
                data: {
                  product_id: text("product_id"),
                  code: text("code"),
                  units: text("units"),
                  raw_kg: text("raw_kg"),
                  warehouse_id: text("warehouse_id"),
                  best_before: text("best_before") || null,
                  expires_on: text("expires_on") || null,
                },
              })
            }}
          >
            <Capped meta={products.data?.meta} />
            <Field
              id="pk-product"
              label={t("packing.product")}
              errors={errors.product_id}
            >
              <select name="product_id" className={selectClass} required>
                {products.data?.data.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field
              id="pk-code"
              label={t("supply.newLotCode")}
              errors={errors.code}
            >
              <Input name="code" required />
            </Field>
            <div className="grid gap-3 sm:grid-cols-2">
              <Field
                id="pk-units"
                label={t("packing.units")}
                errors={errors.units}
              >
                <Input name="units" inputMode="decimal" required />
              </Field>
              <Field
                id="pk-kg"
                label={t("packing.rawKg")}
                errors={errors.raw_kg}
              >
                <Input name="raw_kg" inputMode="decimal" required />
              </Field>
            </div>
            <Capped meta={warehouses.data?.meta} />
            <Field
              id="pk-warehouse"
              label={t("stock.warehouse")}
              errors={errors.warehouse_id}
            >
              <select name="warehouse_id" className={selectClass} required>
                {warehouses.data?.data.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))}
              </select>
            </Field>
            <div className="grid gap-3 sm:grid-cols-2">
              <Field
                id="pk-best"
                label={t("lots.bestBefore")}
                errors={errors.best_before}
              >
                <Input name="best_before" type="date" />
              </Field>
              <Field
                id="pk-expires"
                label={t("lots.expiresOn")}
                errors={errors.expires_on}
              >
                <Input name="expires_on" type="date" />
              </Field>
            </div>
            <ApiError error={pack.error} fallback={t("common.error")} />
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                {t("actions.cancel")}
              </DialogClose>
              <Button type="submit" disabled={pack.isPending}>
                {t("packing.pack")}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
