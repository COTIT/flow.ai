"use client"

import * as React from "react"
import { Package } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getProductsGetQueryKey,
  useProductsGet,
  useProductsImagesList,
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
import { Checkbox } from "@genesy/ui/components/checkbox"
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Textarea } from "@genesy/ui/components/textarea"
import { formatDateTime } from "@genesy/ui/lib/format"
import { Bento, DetailSkeleton, Metric, Tile } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import {
  CardState,
  ListPager,
  ListSkeleton,
  PageHead,
  Segmented,
} from "@/components/list"
import { Capped, Done, Empty, LoadError, useDone } from "@/components/states"
import { StatusBadge } from "@/components/status-badge"

import { ProductImages } from "./images"
import { ProductSizes } from "./sizes"

type Named = { name?: string; bin_code?: string; bin_name?: string } | null

// The API's product statuses (as on the list page); an unknown saved one is kept as an option.
const statuses = ["active", "disabled", "nla"]

export default function ProductPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>
  /** `?done=created`: "New product" just made it. */
  searchParams: Promise<{ done?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const queryClient = useQueryClient()
  const product = useProductsGet(workspaceId, id, undefined, enabled)
  // Same query as the images card; only once the product exists.
  const images = useProductsImagesList(workspaceId, id, {
    query: { enabled: Boolean(workspaceId && product.data?.data) },
  })
  const stock = useStockWithBins(workspaceId, id, enabled)
  const [page, setPage] = React.useState(1)
  // The API's movement types; "decrease" also covers hand corrections, so it reads "Decreases".
  const [type, setType] = React.useState<"" | "decrease" | "set_level">("")
  const movements = useStockMovements(
    workspaceId,
    id,
    { page, type: type || undefined },
    enabled
  )
  const done = useDone(
    React.use(searchParams).done === "created" ? t("done.productCreated") : null
  )
  const update = useProductsUpdate({
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        done.show(t("done.productSaved"))
        return queryClient.invalidateQueries({
          queryKey: getProductsGetQueryKey(workspaceId, id),
        })
      },
    },
  })
  const errors = fieldErrors(update.error)
  const statusLabel = (s: string) =>
    t.has(`products.statuses.${s}`) ? t(`products.statuses.${s}`) : s
  const place = (s: { warehouse: unknown; bin: unknown }) =>
    [
      (s.warehouse as Named)?.name,
      (s.bin as Named)?.bin_name ?? (s.bin as Named)?.bin_code,
    ]
      .filter(Boolean)
      .join(" · ")
  const columns = [
    { label: t("lots.eventAt") },
    { label: t("stock.type") },
    { label: t("stock.warehouse") },
    { label: t("stock.reason") },
    { label: t("stock.quantityChange"), right: true },
    { label: t("stock.after"), right: true },
  ]

  // Stock too: its tile's height (rows or "none yet") would move Sizes and Images below it.
  if (product.isPending || stock.isPending) return <DetailSkeleton />
  if (product.isError)
    return (
      <LoadError
        error={product.error}
        onRetry={() => void product.refetch()}
        backHref="/products"
      />
    )
  const p = product.data.data
  const cover = images.data?.data[0]?.link?.url

  return (
    <>
      <div className="flex items-center gap-4 max-md:items-start">
        <span
          aria-hidden="true"
          className="inline-flex size-16 shrink-0 items-center justify-center overflow-hidden rounded-[18px] border border-(--hair) bg-(--glass-subtle) text-muted-foreground"
        >
          {cover ? (
            <img
              src={cover}
              alt=""
              referrerPolicy="no-referrer"
              className="size-full object-cover"
            />
          ) : (
            <Package className="size-6" strokeWidth={1.75} />
          )}
        </span>
        {/* `title` first: it is the first read of the product (the crash spec expects 'name'). */}
        <PageHead
          className="min-w-0 flex-1"
          back={{ href: "/products", label: t("products.title") }}
          title={p.name}
          description={
            <span className="font-mono tracking-[0.04em]">
              {[p.product_nr, p.reference].filter(Boolean).join(" · ")}
            </span>
          }
          actions={
            <>
              {p.status ? (
                <StatusBadge status={p.status} label={statusLabel(p.status)} />
              ) : null}
              {/* Native reset puts every field back to the saved value. */}
              <Button
                type="reset"
                form="product-form"
                variant="outline"
                onClick={() => update.reset()}
              >
                {t("actions.discard")}
              </Button>
              <Button
                type="submit"
                form="product-form"
                disabled={update.isPending}
              >
                {t("actions.saveChanges")}
              </Button>
            </>
          }
        />
      </div>
      <Done message={done.message} />
      <Bento>
        <Tile
          id="product-details"
          title={t("products.details")}
          span="col-span-8"
          action={
            <span className="text-xs text-muted-foreground">
              {t("products.serverChecked")}
            </span>
          }
        >
          <form
            id="product-form"
            key={p.last_updated_at}
            className="flex flex-col gap-4 max-md:gap-3"
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
                  // An empty field clears them; the API checks the count and length.
                  labels: String(f.get("labels") ?? "")
                    .split(",")
                    .map((l) => l.trim())
                    .filter(Boolean),
                },
              })
            }}
          >
            <div className="grid gap-4 max-md:gap-3 md:grid-cols-2">
              <Field
                id="p-name"
                label={t("products.name")}
                errors={errors.name}
              >
                <Input name="name" defaultValue={p.name} required />
              </Field>
              <Field
                id="p-reference"
                label={t("products.reference")}
                errors={errors.reference}
              >
                <Input name="reference" defaultValue={p.reference ?? ""} />
              </Field>
              <div className="flex min-w-0 flex-col gap-1.5">
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
                {p.price_incl ? (
                  <p className="text-xs text-muted-foreground">
                    {t("products.priceInclHint", { price: p.price_incl })}
                  </p>
                ) : null}
              </div>
              <Field
                id="p-status"
                label={t("products.status")}
                errors={errors.status}
              >
                <select
                  name="status"
                  defaultValue={p.status ?? ""}
                  className={selectClass}
                >
                  {[...new Set([...statuses, p.status].filter(Boolean))].map(
                    (s) => (
                      <option key={s} value={s}>
                        {statusLabel(s)}
                      </option>
                    )
                  )}
                </select>
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
            <div className="flex min-w-0 flex-col gap-1.5">
              <Field
                id="p-labels"
                label={t("products.labels")}
                // The API names the list (`labels`) or one item (`labels.3`).
                errors={Object.entries(errors).flatMap(([k, v]) =>
                  k.split(".")[0] === "labels" ? v : []
                )}
              >
                <Input name="labels" defaultValue={p.labels.join(", ")} />
              </Field>
              <p className="text-xs text-muted-foreground">
                {t("products.labelsHint")}
              </p>
            </div>
            <label className="flex items-center gap-2 text-sm max-md:min-h-11">
              {/* The shared checkbox: its form value is "on", and its hit area is 44 px on a phone. */}
              <Checkbox name="store_only" defaultChecked={p.store_only} />
              {t("products.storeOnly")}
            </label>
            <ApiError error={update.error} fallback={t("common.error")} />
          </form>
        </Tile>

        <div className="col-span-4 flex min-w-0 flex-col gap-4 max-md:gap-3">
          <Tile
            id="product-stock"
            title={t("stock.title")}
            span=""
            action={
              <MutateStock
                productId={id}
                onDone={() => {
                  done.show(t("done.stockChanged"))
                  for (const queryKey of [
                    getStockWithBinsQueryKey(workspaceId, id),
                    getStockMovementsQueryKey(workspaceId, id),
                    getProductsGetQueryKey(workspaceId, id),
                  ])
                    void queryClient.invalidateQueries({ queryKey })
                }}
              />
            }
          >
            {/* Totals as the API sums them; never added up here. */}
            <div className="grid gap-2.5 md:grid-cols-2">
              <Metric label={t("stock.onHand")}>
                {p.total_available_stock ?? "—"}
              </Metric>
              <Metric label={t("stock.reserved")}>
                {p.total_reserved_stock ?? "—"}
              </Metric>
            </div>
            {stock.isError ? (
              <LoadError
                error={stock.error}
                onRetry={() => void stock.refetch()}
              />
            ) : stock.data.data.length === 0 ? (
              <Empty title={t("stock.empty")} />
            ) : (
              <div className="flex flex-col gap-2 text-sm">
                {stock.data.data.map((s) => (
                  <div key={s.id} className="flex justify-between gap-3">
                    <span className="min-w-0">{place(s)}</span>
                    <span className="font-semibold tabular-nums">
                      {s.stock_lvl}
                    </span>
                  </div>
                ))}
                {stock.data.data.map((s, _, rows) => (
                  <div
                    key={`min-${s.id}`}
                    className="flex justify-between gap-3 text-muted-foreground"
                  >
                    <span className="min-w-0">
                      {t("stock.minimum")}
                      {rows.length > 1 ? ` · ${place(s)}` : ""}
                    </span>
                    <span className="tabular-nums">
                      {s.minimal_stock_level}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </Tile>
          <ProductSizes productId={id} done={done} />
          <ProductImages productId={id} done={done} />
        </div>

        <Tile
          id="product-movements"
          title={t("stock.movements")}
          span="col-span-12"
          flush
          action={
            <Segmented
              label={t("stock.movementType")}
              value={type}
              options={[
                { value: "", label: t("stock.typeAll") },
                { value: "decrease", label: t("stock.typeDecrease") },
                { value: "set_level", label: t("stock.typeCount") },
              ]}
              onChange={(v) => {
                setType(v)
                setPage(1)
              }}
            />
          }
        >
          {movements.isPending ? (
            <ListSkeleton columns={columns} />
          ) : movements.isError ? (
            <CardState>
              <LoadError
                error={movements.error}
                onRetry={() => void movements.refetch()}
              />
            </CardState>
          ) : movements.data.data.length === 0 ? (
            <CardState>
              <Empty title={t("stock.noMovements")} />
            </CardState>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  {columns.map((c) => (
                    <TableHead
                      key={c.label}
                      className={c.right ? "text-right" : undefined}
                    >
                      {c.label}
                    </TableHead>
                  ))}
                </TableRow>
              </TableHeader>
              <TableBody>
                {movements.data.data.map((m) => (
                  <TableRow key={m.id}>
                    <TableCell className="tabular-nums">
                      {formatDateTime(m.created_at, locale)}
                    </TableCell>
                    <TableCell data-m="sub">
                      <StatusBadge
                        status={m.movement_type}
                        tone="slate"
                        label={t(`stock.movementTypes.${m.movement_type}`)}
                      />
                    </TableCell>
                    <TableCell data-m="hide">
                      {(m.relationships?.warehouse as Named)?.name}
                    </TableCell>
                    <TableCell data-m="hide" className="text-muted-foreground">
                      {m.reason ?? "—"}
                    </TableCell>
                    <TableCell
                      data-m="right"
                      className="text-right font-semibold tabular-nums"
                    >
                      {m.quantity_change}
                    </TableCell>
                    <TableCell
                      data-m="hide"
                      className="text-right tabular-nums"
                    >
                      {m.quantity_after}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
          {movements.data?.data.length ? (
            <div className="border-t border-(--hair) px-5 py-3.5 max-md:px-4 max-md:py-3">
              <ListPager meta={movements.data.meta} onPage={setPage} />
            </div>
          ) : null}
        </Tile>
      </Bento>
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
      <DialogTrigger render={<Button variant="outline" size="sm" />}>
        {t("stock.change")}
      </DialogTrigger>
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
          <Capped meta={warehouses.data?.meta} />
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
