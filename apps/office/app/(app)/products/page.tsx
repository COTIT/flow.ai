"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { Package, Search } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import { useProductCategoriesList } from "@genesy/api-client/endpoints/product-categories"
import { useProductsList } from "@genesy/api-client/endpoints/products"
import { buttonVariants } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
  PillLinks,
} from "@/components/list"
import { selectClass } from "@/components/field"
import { Empty, LoadError } from "@/components/states"
import { StatusBadge } from "@/components/status-badge"
import { useCan } from "@/lib/can"

import { CreateProduct } from "./create-product"

// The API's product statuses; with none asked it lists `active`.
const statuses = ["active", "disabled", "nla"] as const

export default function ProductsPage({
  searchParams,
}: {
  searchParams: Promise<{
    q?: string
    page?: string
    status?: string
    category?: string
    /** `?new=1` opens the create dialog. */
    new?: string
  }>
}) {
  const t = useTranslations()
  const router = useRouter()
  const { q, page, status, category, new: startNew } = React.use(searchParams)
  const canCreate = useCan("product.create")
  const workspaceId = useSession((s) => s.workspaceId)
  const products = useProductsList(
    workspaceId ?? "",
    {
      q: q || undefined,
      status: status || undefined,
      category_id: category || undefined,
      page: Number(page) || undefined,
    },
    { query: { enabled: Boolean(workspaceId) } }
  )
  // shortcut: first 100 categories for names and the filter; add `category {id,name}` to the
  // product resource when a workspace has more.
  const categories = useProductCategoriesList(
    workspaceId ?? "",
    { page_size: 100 },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const categoryName = (id: string | null) =>
    categories.data?.data.find((c) => c.id === id)?.name
  const href = (next: {
    q?: string
    page?: number
    status?: string
    category?: string
  }) => {
    const params = new URLSearchParams()
    const query = next.q ?? q
    const st = next.status ?? status
    const cat = next.category ?? category
    if (query) params.set("q", query)
    if (st && st !== "active") params.set("status", st)
    if (cat) params.set("category", cat)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    return `?${params}`
  }
  const columns = [
    { label: t("products.name") },
    { label: t("products.number") },
    { label: t("products.reference") },
    { label: t("products.category") },
    { label: t("products.status") },
    { label: t("products.price"), right: true },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.sell")}
        title={t("products.title")}
        actions={
          <>
            <Link
              href="/stock"
              className={buttonVariants({ variant: "outline" })}
            >
              {t("nav.stock")}
            </Link>
            <CreateProduct
              disabled={!canCreate}
              startOpen={startNew === "1"}
              onCreated={(id) => router.push(`/products/${id}?done=created`)}
            />
          </>
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
                router.push(
                  href({
                    q: String(new FormData(e.currentTarget).get("q") ?? ""),
                    page: 1,
                  })
                )
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
                aria-label={t("products.search")}
                placeholder={t("products.search")}
                className="pl-10"
              />
            </form>
            {categories.data?.data.length ? (
              <select
                aria-label={t("products.category")}
                value={category ?? ""}
                onChange={(e) =>
                  router.push(href({ category: e.target.value, page: 1 }))
                }
                className={`${selectClass} w-auto max-md:w-full`}
              >
                <option value="">{t("products.allCategories")}</option>
                {categories.data.data.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            ) : null}
            <div className="max-md:hidden">
              <PillLinks
                label={t("products.status")}
                links={statuses.map((s) => ({
                  href: href({ status: s, page: 1 }),
                  label: t(`products.statuses.${s}`),
                  active: (status || "active") === s,
                }))}
              />
            </div>
          </>
        }
        count={
          products.data
            ? t("products.count", { total: products.data.meta.total })
            : null
        }
        pager={
          products.data?.data.length ? (
            <ListPager
              meta={products.data.meta}
              onPage={(p) => router.push(href({ page: p }))}
            />
          ) : null
        }
      >
        {products.isPending ? (
          <ListSkeleton columns={columns} rowHeight="h-10" />
        ) : products.isError ? (
          <CardState>
            <LoadError
              error={products.error}
              onRetry={() => void products.refetch()}
            />
          </CardState>
        ) : products.data.data.length === 0 ? (
          <CardState>
            <Empty
              title={t("products.empty")}
              onClear={q || status || category ? () => router.push("?") : undefined}
            />
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
              {products.data.data.map((p) => (
                <TableRow key={p.id}>
                  <TableCell>
                    <span className="flex items-center gap-3">
                      <span
                        aria-hidden="true"
                        className="inline-flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-[10px] border border-(--hair) bg-(--glass-subtle) text-muted-foreground max-md:hidden"
                      >
                        {p.thumbnail ? (
                          <img
                            src={p.thumbnail.url}
                            alt=""
                            referrerPolicy="no-referrer"
                            className="size-full object-cover"
                          />
                        ) : (
                          <Package className="size-[18px]" strokeWidth={1.75} />
                        )}
                      </span>
                      <Link
                        className="font-semibold underline-offset-4 hover:underline"
                        href={`/products/${p.id}`}
                      >
                        {p.name}
                      </Link>
                    </span>
                  </TableCell>
                  <TableCell
                    data-m="sub"
                    className="font-mono tracking-[0.04em]"
                  >
                    {p.product_nr}
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {p.reference}
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {categoryName(p.category_id)}
                  </TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={p.status}
                      label={
                        t.has(`products.statuses.${p.status}`)
                          ? t(`products.statuses.${p.status}`)
                          : p.status
                      }
                    />
                  </TableCell>
                  {/* The API prints prices as exact strings; shown as-is, never parsed. */}
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {p.price_incl}
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
