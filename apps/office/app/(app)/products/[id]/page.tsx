"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getProductsGetQueryKey,
  useProductsGet,
  useProductsUpdate,
} from "@genesy/api-client/endpoints/products"
import {
  getStockMovementsQueryKey,
  getStockWithBinsQueryKey,
  useStockMovements,
  useStockMutate,
  useStockWithBins,
} from "@genesy/api-client/endpoints/stock"
import { useWarehousesList } from "@genesy/api-client/endpoints/warehouses"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Textarea } from "@genesy/ui/components/textarea"
import { Field, selectClass } from "@/components/field"

type Named = { name?: string; bin_code?: string; bin_name?: string } | null

export default function ProductPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const queryClient = useQueryClient()
  const product = useProductsGet(workspaceId, id, undefined, enabled)
  const stock = useStockWithBins(workspaceId, id, enabled)
  const [page, setPage] = React.useState(1)
  const movements = useStockMovements(workspaceId, id, { page }, enabled)
  const update = useProductsUpdate({
    mutation: {
      onSuccess: () =>
        queryClient.invalidateQueries({
          queryKey: getProductsGetQueryKey(workspaceId, id),
        }),
    },
  })
  const errors = fieldErrors(update.error)

  if (product.isPending)
    return (
      <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
    )
  if (product.isError)
    return <ApiError error={product.error} fallback={t("common.error")} />
  const p = product.data.data

  return (
    <>
      <section className="flex flex-col gap-3">
        <h1 className="heading-2 text-lg">{p.name}</h1>
        <form
          key={p.last_updated_at}
          className="flex max-w-2xl flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const text = (k: string) => String(f.get(k) ?? "") || null
            // Only these keys: the API keeps every key left out.
            update.mutate({
              workspace: workspaceId,
              product: id,
              data: {
                name: text("name"),
                reference: text("reference"),
                price: text("price"),
                status: text("status"),
                short_description: text("short_description"),
                description: text("description"),
                store_only: f.get("store_only") === "on",
              },
            })
          }}
        >
          <Field id="p-name" label={t("products.name")} errors={errors.name}>
            <Input name="name" defaultValue={p.name} required />
          </Field>
          <div className="grid gap-3 sm:grid-cols-3">
            <Field
              id="p-reference"
              label={t("products.reference")}
              errors={errors.reference}
            >
              <Input name="reference" defaultValue={p.reference ?? ""} />
            </Field>
            {/* Decimal string as the API prints it; sent back unparsed. */}
            <Field
              id="p-price"
              label={t("products.netPrice")}
              errors={errors.price}
            >
              <Input
                name="price"
                inputMode="decimal"
                defaultValue={p.price ?? ""}
              />
            </Field>
            <Field
              id="p-status"
              label={t("products.status")}
              errors={errors.status}
            >
              <Input name="status" defaultValue={p.status ?? ""} />
            </Field>
          </div>
          <Field
            id="p-short"
            label={t("products.shortDescription")}
            errors={errors.short_description}
          >
            <Input
              name="short_description"
              defaultValue={p.short_description ?? ""}
            />
          </Field>
          <Field
            id="p-description"
            label={t("products.description")}
            errors={errors.description}
          >
            <Textarea name="description" defaultValue={p.description ?? ""} />
          </Field>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              name="store_only"
              defaultChecked={p.store_only}
            />
            {t("products.storeOnly")}
          </label>
          <ApiError error={update.error} fallback={t("common.error")} />
          {update.isSuccess ? (
            <p role="status" className="text-sm text-muted-foreground">
              {t("actions.saved")}
            </p>
          ) : null}
          <Button
            type="submit"
            className="self-start"
            disabled={update.isPending}
          >
            {t("actions.save")}
          </Button>
        </form>
      </section>

      <section className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="mr-auto heading-2 text-base">
            {t("stock.perWarehouse")}
          </h2>
          <MutateStock
            productId={id}
            onDone={() => {
              for (const queryKey of [
                getStockWithBinsQueryKey(workspaceId, id),
                getStockMovementsQueryKey(workspaceId, id),
              ])
                void queryClient.invalidateQueries({ queryKey })
            }}
          />
        </div>
        {stock.isPending ? (
          <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
        ) : stock.isError ? (
          <ApiError error={stock.error} fallback={t("common.error")} />
        ) : stock.data.data.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("stock.empty")}</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("stock.warehouse")}</TableHead>
                <TableHead>{t("stock.bin")}</TableHead>
                <TableHead className="text-right">{t("stock.level")}</TableHead>
                <TableHead className="text-right">
                  {t("stock.reserved")}
                </TableHead>
                <TableHead className="text-right">
                  {t("stock.minimum")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {stock.data.data.map((s) => (
                <TableRow key={s.id}>
                  <TableCell>{(s.warehouse as Named)?.name}</TableCell>
                  <TableCell>
                    {(s.bin as Named)?.bin_name ??
                      (s.bin as Named)?.bin_code ??
                      "—"}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {s.stock_lvl}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {s.reserved}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {s.minimal_stock_level}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="heading-2 text-base">{t("stock.movements")}</h2>
        {movements.isPending ? (
          <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
        ) : movements.isError ? (
          <ApiError error={movements.error} fallback={t("common.error")} />
        ) : movements.data.data.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("stock.noMovements")}
          </p>
        ) : (
          <>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("lots.eventAt")}</TableHead>
                  <TableHead>{t("stock.type")}</TableHead>
                  <TableHead className="text-right">
                    {t("stock.quantityChange")}
                  </TableHead>
                  <TableHead className="text-right">
                    {t("stock.after")}
                  </TableHead>
                  <TableHead>{t("stock.reason")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {movements.data.data.map((m) => (
                  <TableRow key={m.id}>
                    <TableCell className="tabular-nums">
                      {new Date(m.created_at).toLocaleString()}
                    </TableCell>
                    <TableCell>{m.movement_type}</TableCell>
                    <TableCell className="text-right tabular-nums">
                      {m.quantity_change}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {m.quantity_after}
                    </TableCell>
                    <TableCell>{m.reason}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <Pager
              page={movements.data.meta.current_page}
              lastPage={movements.data.meta.last_page}
              summary={t("pager.summary", {
                from: movements.data.meta.from ?? 0,
                to: movements.data.meta.to ?? 0,
                total: movements.data.meta.total,
              })}
              previous={t("pager.previous")}
              next={t("pager.next")}
              onPage={setPage}
            />
          </>
        )}
      </section>
    </>
  )
}

/** Add to or take from one warehouse's stock; the change is a signed decimal string. */
function MutateStock({
  productId,
  onDone,
}: {
  productId: string
  onDone: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const warehouses = useWarehousesList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: open && Boolean(workspaceId) } }
  )
  const mutate = useStockMutate({
    mutation: {
      onSuccess: () => {
        onDone()
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
      <DialogTrigger render={<Button />}>{t("stock.change")}</DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            mutate.mutate({
              workspace: workspaceId,
              product: productId,
              data: {
                warehouse_id: String(f.get("warehouse_id") ?? ""),
                mutation_value: String(f.get("mutation_value") ?? ""),
                reason: String(f.get("reason") ?? "") || null,
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("stock.change")}</DialogTitle>
          </DialogHeader>
          <Field
            id="m-warehouse"
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
          <Field
            id="m-value"
            label={t("stock.changeHint")}
            errors={errors.mutation_value}
          >
            <Input name="mutation_value" inputMode="decimal" required />
          </Field>
          <Field id="m-reason" label={t("stock.reason")} errors={errors.reason}>
            <Input name="reason" />
          </Field>
          <ApiError error={mutate.error} fallback={t("common.error")} />
          <DialogFooter>
            <Button type="submit" disabled={mutate.isPending}>
              {t("actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
