"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { Search } from "lucide-react"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getLotsListQueryKey,
  useLotsCreate,
  useLotsList,
} from "@genesy/api-client/endpoints/lots"
import { useProductsList } from "@genesy/api-client/endpoints/products"
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@genesy/ui/components/select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { useRights } from "@/lib/can"
import { Field, selectClass } from "@/components/field"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Capped, Empty, LoadError } from "@/components/states"

import { LotFields, lotStatuses } from "./lot-fields"

export default function LotsPage({
  searchParams,
}: {
  searchParams: Promise<{
    q?: string
    status?: string
    product?: string
    page?: string
    /** `?new=1` (the overview's "New batch") opens the create dialog. */
    new?: string
  }>
}) {
  const t = useTranslations()
  const router = useRouter()
  const { q, status, product, page, new: startNew } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const lots = useLotsList(
    workspaceId,
    {
      q: q || undefined,
      status: status || undefined,
      product_id: product || undefined,
      page: Number(page) || undefined,
    },
    { query: { enabled: Boolean(workspaceId) } }
  )
  // Product names, the product filter and New batch need the product list (and New batch the
  // right to change products); a member without them sees the batches alone.
  const { may } = useRights()
  const readProducts = may("product.list")
  // shortcut: first 100 products only; swap for a search picker when the catalog grows past that.
  const products = useProductsList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: Boolean(workspaceId) && readProducts } }
  )
  const productName = (id: string) =>
    readProducts
      ? (products.data?.data.find((p) => p.id === id)?.name ?? "")
      : "—"
  const go = (next: {
    q?: string
    status?: string
    product?: string
    page?: number
  }) => {
    const params = new URLSearchParams()
    const query = next.q ?? q
    const st = next.status ?? status
    const pr = next.product ?? product
    if (query) params.set("q", query)
    if (st) params.set("status", st)
    if (pr) params.set("product", pr)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }

  const columns = [
    { label: t("lots.code") },
    { label: t("lots.product") },
    { label: t("lots.status") },
    { label: t("lots.producedOn") },
    { label: t("lots.bestBefore") },
    { label: t("lots.publicCode") },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.trace")}
        title={t("lots.title")}
        actions={
          readProducts && may("product.update") ? (
            <CreateLot
              products={products.data?.data ?? []}
              productsMeta={products.data?.meta}
              startOpen={startNew === "1"}
              onCreated={(id) => router.push(`/lots/${id}?done=created`)}
            />
          ) : null
        }
      />
      <ListCard
        filters={
          <>
            <form
              role="search"
              className="relative w-full max-w-[360px]"
              onSubmit={(e) => {
                e.preventDefault()
                go({ q: String(new FormData(e.currentTarget).get("q") ?? "") })
              }}
            >
              <Search
                aria-hidden="true"
                strokeWidth={1.75}
                className="pointer-events-none absolute top-1/2 left-3.5 size-[18px] -translate-y-1/2 text-muted-foreground"
              />
              <Input
                name="q"
                type="search"
                defaultValue={q}
                aria-label={t("lots.search")}
                placeholder={t("lots.search")}
                className="pl-10"
              />
            </form>
            <FilterSelect
              label={t("lots.status")}
              value={status}
              items={[
                { value: "", label: t("lots.allStatuses") },
                ...lotStatuses.map((s) => ({
                  value: s,
                  label: t(`lots.statuses.${s}`),
                })),
              ]}
              onChange={(v) => go({ status: v })}
            />
            {readProducts ? (
              <FilterSelect
                label={t("lots.product")}
                value={product}
                items={[
                  { value: "", label: t("lots.allProducts") },
                  ...(products.data?.data ?? []).map((p) => ({
                    value: p.id,
                    label: p.name,
                  })),
                ]}
                onChange={(v) => go({ product: v })}
              />
            ) : null}
          </>
        }
        count={
          lots.data ? t("lots.count", { total: lots.data.meta.total }) : null
        }
        pager={
          lots.data?.data.length ? (
            <ListPager meta={lots.data.meta} onPage={(p) => go({ page: p })} />
          ) : null
        }
      >
        {lots.isPending ? (
          <ListSkeleton columns={columns} />
        ) : lots.isError ? (
          <CardState>
            <LoadError error={lots.error} onRetry={() => void lots.refetch()} />
          </CardState>
        ) : lots.data.data.length === 0 ? (
          <CardState>
            <Empty
              title={t("lots.empty")}
              onClear={
                q || status || product ? () => router.push("?") : undefined
              }
            />
          </CardState>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                {columns.map((c) => (
                  <TableHead key={c.label}>{c.label}</TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {lots.data.data.map((lot) => (
                <TableRow key={lot.id}>
                  <TableCell>
                    <Link
                      className="font-mono font-semibold tracking-[0.04em] underline-offset-4 hover:underline"
                      href={`/lots/${lot.id}`}
                    >
                      {lot.code}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub">{productName(lot.product_id)}</TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={lot.status}
                      label={
                        t.has(`lots.statuses.${lot.status}`)
                          ? t(`lots.statuses.${lot.status}`)
                          : lot.status
                      }
                    />
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {lot.produced_on}
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {lot.best_before}
                  </TableCell>
                  <TableCell
                    data-m="hide"
                    className="font-mono tracking-[0.04em] text-muted-foreground"
                  >
                    {lot.public_code}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
    </>
  )
}

/** A card-head filter; "" is its "all" option. */
function FilterSelect({
  label,
  value,
  items,
  onChange,
}: {
  label: string
  value?: string
  items: { value: string; label: string }[]
  onChange: (value: string) => void
}) {
  return (
    <Select
      items={items}
      value={value ?? ""}
      onValueChange={(v) => onChange(v ?? "")}
    >
      <SelectTrigger aria-label={label} className="w-[200px] max-md:w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {items.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function CreateLot({
  products,
  productsMeta,
  startOpen = false,
  onCreated,
}: {
  products: { id: string; name: string }[]
  productsMeta?: { total: number; per_page: number }
  startOpen?: boolean
  onCreated: (id: string) => void
}) {
  const t = useTranslations()
  const [open, setOpen] = React.useState(startOpen)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const create = useLotsCreate({
    mutation: {
      onSuccess: ({ data }) => {
        void queryClient.invalidateQueries({
          queryKey: getLotsListQueryKey(workspaceId),
        })
        setOpen(false)
        onCreated(data.id)
      },
    },
  })
  const errors = fieldErrors(create.error)

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) create.reset()
      }}
    >
      <DialogTrigger render={<Button />}>{t("lots.new")}</DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const text = (k: string) => String(f.get(k) ?? "") || null
            create.mutate({
              workspace: workspaceId,
              data: {
                product_id: String(f.get("product_id") ?? ""),
                code: String(f.get("code") ?? ""),
                produced_on: text("produced_on"),
                best_before: text("best_before"),
                expires_on: text("expires_on"),
                origin: text("origin"),
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("lots.new")}</DialogTitle>
          </DialogHeader>
          <Capped meta={productsMeta} />
          <Field
            id="lot-product"
            label={t("lots.product")}
            errors={errors.product_id}
          >
            <select name="product_id" className={selectClass} required>
              {products.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
          <LotFields errors={errors} />
          <ApiError error={create.error} fallback={t("common.error")} />
          <DialogFooter>
            <Button type="submit" disabled={create.isPending}>
              {t("actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
