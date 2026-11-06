"use client"

import * as React from "react"
import Link from "next/link"
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
import { useProductCategoriesList } from "@genesy/api-client/endpoints/product-categories"
import { useWarehousesList } from "@genesy/api-client/endpoints/warehouses"
import type { StockWithBins } from "@genesy/api-client/model"
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
import { Notice } from "@genesy/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Textarea } from "@genesy/ui/components/textarea"
import { formatDateTime, formatQty } from "@genesy/ui/lib/format"
import { Badge } from "@genesy/ui/components/badge"
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
import {
  BatchLink,
  BelowZeroWhy,
  byPlace,
  inlineLink,
  lotOf,
} from "@/components/stock"

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
  /** `?done=created`: "New product" just made it; `?change=<warehouse>` opens Change stock there. */
  searchParams: Promise<{ done?: string; change?: string }>
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
  // shortcut: first 100 categories in the picker; page or search when a workspace has more.
  const categories = useProductCategoriesList(
    workspaceId,
    { page_size: 100 },
    enabled
  )
  const [page, setPage] = React.useState(1)
  // The API's movement types; "decrease" also covers hand corrections, so it reads "Decreases".
  const [type, setType] = React.useState<"" | "decrease" | "set_level">("")
  const movements = useStockMovements(
    workspaceId,
    id,
    { page, type: type || undefined },
    enabled
  )
  const query = React.use(searchParams)
  const done = useDone(
    query.done === "created" ? t("done.productCreated") : null
  )
  // The place Change stock is open at ("" = the first place); null = closed.
  const [changeAt, setChangeAt] = React.useState<string | null>(
    query.change ?? null
  )
  const qty = (v: string) => formatQty(v, locale)
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
              // Sent only once the picker is filled and changed: a failed list never clears it.
              const category = text("category_id")
              const categoryChanged =
                categories.isSuccess && category !== (p.category_id ?? null)
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
                  ...(categoryChanged ? { category_id: category } : {}),
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
              id="p-category"
              label={t("products.category")}
              errors={errors.category_id}
            >
              <select
                name="category_id"
                // Keyed: the saved value applies once the options arrive.
                key={categories.dataUpdatedAt}
                defaultValue={p.category_id ?? ""}
                disabled={!categories.isSuccess}
                className={selectClass}
              >
                <option value="">{t("products.noCategory")}</option>
                {categories.data?.data.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </Field>
            {categories.isError ? (
              <Notice
                variant="warning"
                title={t("products.categoriesFailed")}
              />
            ) : (
              <Capped meta={categories.data?.meta} />
            )}
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
                at={changeAt}
                onAt={setChangeAt}
                rows={stock.data?.data ?? []}
                onDone={(message) => {
                  done.show(message)
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
              <Metric label={t("stock.held")}>
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
              <div className="flex flex-col gap-3 text-sm">
                {byPlace(stock.data.data).map((place) => {
                  const level = (v: string) => (
                    <span className="font-semibold tabular-nums">
                      {/* The sign is read from the API's string, never computed. */}
                      {v.startsWith("-") ? (
                        <Badge variant="red" data-dot="false">
                          {qty(v)}
                        </Badge>
                      ) : (
                        qty(v)
                      )}
                    </span>
                  )
                  const alone = place.rows.length === 1 && place.base
                  return (
                    <div key={place.id} className="flex flex-col gap-1">
                      <div className="flex items-center justify-between gap-3">
                        <span className="min-w-0 font-medium">
                          {place.name}
                        </span>
                        {alone ? level(place.base!.stock_lvl) : null}
                      </div>
                      {alone ? null : (
                        <ul className="flex flex-col gap-1 pl-3">
                          {place.rows.map((r) => {
                            const lot = lotOf(r.lot)
                            const bin = r.bin as Named
                            const binName = bin?.bin_name ?? bin?.bin_code
                            return (
                              <li
                                key={r.id}
                                className="flex items-center justify-between gap-3 text-muted-foreground"
                              >
                                <span className="min-w-0">
                                  {binName ? `${binName} · ` : null}
                                  {lot ? (
                                    <BatchLink lot={lot} />
                                  ) : binName ? null : (
                                    t("stock.noBatch")
                                  )}
                                </span>
                                {level(r.stock_lvl)}
                              </li>
                            )
                          })}
                        </ul>
                      )}
                      {place.base && place.base.minimal_stock_level > 0 ? (
                        <span className="text-muted-foreground">
                          {`${t("stock.minimum")} ${place.base.minimal_stock_level}`}
                        </span>
                      ) : null}
                      {place.rows.some((r) => r.stock_lvl.startsWith("-")) ? (
                        <BelowZeroWhy>
                          <button
                            type="button"
                            className={inlineLink}
                            onClick={() => setChangeAt(place.id)}
                          >
                            {t("stock.correctIt")}
                          </button>
                        </BelowZeroWhy>
                      ) : null}
                    </div>
                  )
                })}
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
                      {m.quantity_change == null ? "—" : qty(m.quantity_change)}
                    </TableCell>
                    <TableCell
                      data-m="hide"
                      className="text-right tabular-nums"
                    >
                      {m.quantity_after == null ? "—" : qty(m.quantity_after)}
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

/** Add to or take from one place's stock (its row without a batch); the change is a signed
 * decimal string. Says what the place holds now, and after saving, where the change went. */
function MutateStock({
  productId,
  at,
  onAt,
  rows,
  onDone,
}: {
  productId: string
  at: string | null
  onAt: (at: string | null) => void
  rows: StockWithBins[]
  onDone: (message: string) => void
}) {
  const t = useTranslations()
  const locale = useLocale()
  const qty = (v: string) => formatQty(v, locale)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const open = at !== null
  const warehouses = useWarehousesList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: open && Boolean(workspaceId) } }
  )
  const chosen = at || warehouses.data?.data[0]?.id || ""
  const place = byPlace(rows).find((p) => p.id === chosen)
  const placeName = warehouses.data?.data.find((w) => w.id === chosen)?.name
  const batches = place?.rows.filter((r) => lotOf(r.lot)) ?? []
  const mutate = useStockMutate({
    mutation: {
      onSuccess: (answer, { data }) => {
        // Place and levels as the API answers them, from the change's own transaction.
        const value = data.mutation_value.trim()
        const taken = value.startsWith("-")
        onDone(
          answer.after && answer.warehouse
            ? t(taken ? "done.stockTaken" : "done.stockAdded", {
                qty: qty(value.replace(/^[-+]/, "")),
                place: answer.warehouse.name,
                now: qty(answer.after),
              })
            : t("done.stockUnmoved")
        )
        onAt(null)
      },
    },
  })
  const errors = fieldErrors(mutate.error)

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        onAt(o ? "" : null)
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
            <select
              name="warehouse_id"
              className={selectClass}
              required
              value={chosen}
              onChange={(e) => onAt(e.target.value)}
            >
              {warehouses.data?.data.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </Field>
          {placeName ? (
            <div className="flex flex-col gap-1 text-sm" aria-live="polite">
              <p>
                {place?.base
                  ? t("stock.nowAt", {
                      place: placeName,
                      qty: qty(place.base.stock_lvl),
                    })
                  : t("stock.noneAt", { place: placeName })}
              </p>
              {batches.length ? (
                <>
                  <p className="text-muted-foreground">
                    {t("stock.batchesApart")}
                  </p>
                  <ul className="flex flex-col">
                    {batches.map((r) => {
                      const lot = lotOf(r.lot)!
                      return (
                        <li key={r.id}>
                          <Link className={inlineLink} href={`/lots/${lot.id}`}>
                            {`${t("stock.batch", { code: lot.code })}: ${qty(r.stock_lvl)}`}
                          </Link>
                        </li>
                      )
                    })}
                  </ul>
                </>
              ) : null}
            </div>
          ) : null}
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
