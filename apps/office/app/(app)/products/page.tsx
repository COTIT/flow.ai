"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useProductsList } from "@genesy/api-client/endpoints/products"
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

export default function ProductsPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string; page?: string }>
}) {
  const t = useTranslations()
  const router = useRouter()
  const { q, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId)
  const products = useProductsList(
    workspaceId ?? "",
    { q: q || undefined, page: Number(page) || undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const go = (next: { q?: string; page?: number }) => {
    const params = new URLSearchParams()
    const query = next.q ?? q
    if (query) params.set("q", query)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }

  return (
    <section className="flex flex-col gap-3">
      <h1 className="heading-2 text-lg">{t("products.title")}</h1>
      <form
        role="search"
        onSubmit={(e) => {
          e.preventDefault()
          go({ q: String(new FormData(e.currentTarget).get("q") ?? "") })
        }}
      >
        <Input
          name="q"
          type="search"
          defaultValue={q}
          aria-label={t("products.search")}
          placeholder={t("products.search")}
          className="max-w-sm"
        />
      </form>
      {products.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : products.isError ? (
        <p role="alert" className="text-sm text-destructive">
          {t("common.error")}
        </p>
      ) : products.data.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("products.empty")}</p>
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("products.name")}</TableHead>
                <TableHead>{t("products.number")}</TableHead>
                <TableHead>{t("products.reference")}</TableHead>
                <TableHead className="text-right">
                  {t("products.price")}
                </TableHead>
                <TableHead>{t("products.status")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {products.data.data.map((p) => (
                <TableRow key={p.id}>
                  <TableCell>
                    <Link
                      className="font-medium underline-offset-4 hover:underline"
                      href={`/products/${p.id}`}
                    >
                      {p.name}
                    </Link>
                  </TableCell>
                  <TableCell>{p.product_nr}</TableCell>
                  <TableCell>{p.reference}</TableCell>
                  {/* The API prints prices as exact strings; shown as-is, never parsed. */}
                  <TableCell className="text-right tabular-nums">
                    {p.price_incl}
                  </TableCell>
                  <TableCell>{p.status}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pager
            page={products.data.meta.current_page}
            lastPage={products.data.meta.last_page}
            summary={t("pager.summary", {
              from: products.data.meta.from ?? 0,
              to: products.data.meta.to ?? 0,
              total: products.data.meta.total,
            })}
            previous={t("pager.previous")}
            next={t("pager.next")}
            onPage={(p) => go({ page: p })}
          />
        </>
      )}
    </section>
  )
}
