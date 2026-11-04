"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getLotsEventsQueryKey,
  getLotsStockQueryKey,
  useLotsStock,
  useLotsStockMutate,
} from "@genesy/api-client/endpoints/lots"
import { useWarehousesList } from "@genesy/api-client/endpoints/warehouses"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Checkbox } from "@genesy/ui/components/checkbox"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Input } from "@genesy/ui/components/input"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { SubTile } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { Capped, LoadError } from "@/components/states"

/**
 * Put stock into this batch or take it out, per warehouse (as Change stock on a product).
 * Shows the free stock first; the API checks the change, so no sums are done here.
 */
export function StockDialog({
  lotId,
  code,
  onChanged,
}: {
  lotId: string
  code: string
  onChanged: () => void
}) {
  const t = useTranslations()
  const queryClient = useQueryClient()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const enabled = { query: { enabled: open && Boolean(workspaceId) } }
  const stock = useLotsStock(workspaceId, lotId, enabled)
  // shortcut: first 100 warehouses in the picker; add search when accounts have more.
  const warehouses = useWarehousesList(workspaceId, { page_size: 100 }, enabled)
  const name = (id: string) =>
    warehouses.data?.data.find((w) => w.id === id)?.name ?? id
  const mutate = useLotsStockMutate({
    mutation: {
      onSuccess: () => {
        for (const queryKey of [
          getLotsStockQueryKey(workspaceId, lotId),
          getLotsEventsQueryKey(workspaceId, lotId),
        ])
          void queryClient.invalidateQueries({ queryKey })
        onChanged()
        setOpen(false)
      },
    },
  })
  const errors = fieldErrors(mutate.error)

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) mutate.reset()
      }}
    >
      <DialogTrigger render={<Button variant="outline" />}>
        {t("stock.change")}
      </DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const text = (k: string) => String(f.get(k) ?? "").trim()
            mutate.mutate({
              workspace: workspaceId,
              lot: lotId,
              data: {
                warehouse_id: text("warehouse_id"),
                quantity: text("quantity"),
                reason: text("reason") || null,
                is_correction: f.get("is_correction") === "on",
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("lotStock.title", { code })}</DialogTitle>
          </DialogHeader>
          {stock.isPending || warehouses.isPending ? (
            <Skeleton aria-busy="true" className="h-[88px] rounded-2xl" />
          ) : stock.isError ? (
            <LoadError
              error={stock.error}
              onRetry={() => void stock.refetch()}
            />
          ) : stock.data.data.length === 0 ? (
            <EmptyState title={t("lotStock.empty")} />
          ) : (
            <div className="flex flex-col gap-2">
              {stock.data.data.map((s) => (
                <div
                  key={s.warehouse_id}
                  role="group"
                  aria-label={name(s.warehouse_id)}
                >
                  <SubTile className="flex flex-col gap-2 px-3 py-2.5">
                    <p className="text-sm font-semibold">
                      {name(s.warehouse_id)}
                    </p>
                    <dl className="grid grid-cols-3 gap-2 text-xs">
                      {(
                        [
                          ["stock.level", s.level],
                          ["stock.reserved", s.reserved],
                          ["lotStock.available", s.available],
                        ] as const
                      ).map(([label, value]) => (
                        <div key={label}>
                          <dt className="text-muted-foreground">{t(label)}</dt>
                          <dd className="text-sm font-semibold tabular-nums">
                            {value}
                          </dd>
                        </div>
                      ))}
                    </dl>
                  </SubTile>
                </div>
              ))}
            </div>
          )}
          <Capped meta={warehouses.data?.meta} />
          <Field
            id="ls-warehouse"
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
          <div className="flex flex-col gap-1.5">
            <Field
              id="ls-quantity"
              label={t("lotStock.quantity")}
              errors={errors.quantity}
            >
              <Input name="quantity" inputMode="decimal" required />
            </Field>
            <p className="text-xs text-muted-foreground">
              {t("lotStock.quantityHint")}
            </p>
          </div>
          <Field id="ls-reason" label={t("stock.reason")} errors={errors.reason}>
            <Input name="reason" maxLength={255} />
          </Field>
          <label className="flex items-center gap-2 text-sm max-md:min-h-11">
            {/* The shared checkbox: its form value is "on", and its hit area is 44 px on a phone. */}
            <Checkbox name="is_correction" />
            {t("lotStock.correction")}
          </label>
          {errors.is_correction ? (
            <p className="text-sm text-destructive">
              {errors.is_correction.join(" ")}
            </p>
          ) : null}
          <ApiError error={mutate.error} fallback={t("common.error")} />
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              {t("actions.cancel")}
            </DialogClose>
            <Button type="submit" disabled={mutate.isPending}>
              {t("actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
