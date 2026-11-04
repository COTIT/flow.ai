"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"
import { Search } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import { useSearchQuick } from "@genesy/api-client/endpoints/search"
import { EmptyState } from "@genesy/ui/components/empty-state"
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
import { formatMoney } from "@genesy/ui/lib/money"
import { Tile } from "@/components/bento"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { LoadError } from "@/components/states"
import { StatusBadge } from "@/components/status-badge"

const titleLink = "font-semibold underline-offset-4 hover:underline"

/**
 * Quick search results (List template): one card per group, each a table like its list page.
 * A group is null without its list permission, so it is left out like an empty one. Relations
 * have no office page to link to, so they are not shown.
 */
export default function SearchPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const q = (React.use(searchParams).q ?? "").trim()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  // The API takes 3 to 100 characters; shorter asks for more instead of a 422.
  const valid = q.length >= 3
  const results = useSearchQuick(
    workspaceId,
    { q, limit: 10 },
    { query: { enabled: Boolean(workspaceId) && valid } }
  )
  const r = results.data?.data
  const orderStatus = (s: string) =>
    t.has(`orders.statuses.${s}`) ? t(`orders.statuses.${s}`) : s
  const head = (labels: string[], right = -1) => (
    <TableHeader>
      <TableRow>
        {labels.map((l, i) => (
          <TableHead key={l} className={i === right ? "text-right" : undefined}>
            {l}
          </TableHead>
        ))}
      </TableRow>
    </TableHeader>
  )

  const groups = r
    ? [
        r.products?.length ? (
          <Tile
            key="p"
            id="search-products"
            title={t("nav.products")}
            span=""
            flush
          >
            <Table>
              {head([
                t("products.name"),
                t("products.number"),
                t("search.ean"),
              ])}
              <TableBody>
                {r.products.map((p) => (
                  <TableRow key={p.id}>
                    <TableCell>
                      <Link className={titleLink} href={`/products/${p.id}`}>
                        {p.name}
                      </Link>
                    </TableCell>
                    <TableCell
                      data-m="sub"
                      className="font-mono tracking-[0.04em]"
                    >
                      {p.product_nr}
                    </TableCell>
                    <TableCell
                      data-m="hide"
                      className="font-mono text-muted-foreground"
                    >
                      {p.ean13 ?? "—"}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </Tile>
        ) : null,
        r.orders?.length ? (
          <Tile
            key="o"
            id="search-orders"
            title={t("nav.orders")}
            span=""
            flush
          >
            <Table>
              {head(
                [
                  t("orders.number"),
                  t("orders.reference"),
                  t("orders.status"),
                  t("invoices.total"),
                ],
                3
              )}
              <TableBody>
                {r.orders.map((o) => (
                  <TableRow key={o.id}>
                    <TableCell>
                      <Link className={titleLink} href={`/orders/${o.id}`}>
                        {o.number}
                      </Link>
                    </TableCell>
                    <TableCell data-m="sub">{o.reference ?? "—"}</TableCell>
                    <TableCell data-m="badge">
                      <StatusBadge
                        status={o.status}
                        label={orderStatus(o.status)}
                      />
                    </TableCell>
                    {/* The server's total in minor units; formatted, never summed. */}
                    <TableCell
                      data-m="right"
                      className="text-right font-semibold tabular-nums"
                    >
                      {formatMoney(o.total, o.decimals, o.currency, locale)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </Tile>
        ) : null,
        r.invoices?.length ? (
          <Tile
            key="i"
            id="search-invoices"
            title={t("nav.invoices")}
            span=""
            flush
          >
            <Table>
              {head([
                t("invoices.number"),
                t("invoices.customer"),
                t("invoices.status"),
              ])}
              <TableBody>
                {r.invoices.map((inv) => (
                  <TableRow key={inv.id}>
                    <TableCell>
                      <Link className={titleLink} href={`/invoices/${inv.id}`}>
                        {inv.number}
                      </Link>
                    </TableCell>
                    <TableCell data-m="sub">
                      {inv.customer_name ?? "—"}
                    </TableCell>
                    <TableCell data-m="badge">
                      <StatusBadge
                        status={inv.status}
                        label={t(`invoices.statuses.${inv.status}`)}
                      />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </Tile>
        ) : null,
        r.lots?.length ? (
          <Tile key="l" id="search-lots" title={t("nav.lots")} span="" flush>
            <Table>
              {head([t("lots.code"), t("lots.product"), t("lots.publicCode")])}
              <TableBody>
                {r.lots.map((l) => (
                  <TableRow key={l.id}>
                    <TableCell>
                      <Link
                        className={`${titleLink} font-mono tracking-[0.04em]`}
                        href={`/lots/${l.id}`}
                      >
                        {l.code}
                      </Link>
                    </TableCell>
                    <TableCell data-m="sub">{l.product_name}</TableCell>
                    <TableCell
                      data-m="hide"
                      className="font-mono text-muted-foreground"
                    >
                      {l.public_code}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </Tile>
        ) : null,
      ].filter(Boolean)
    : []

  return (
    <>
      <PageHead
        title={t("search.title")}
        description={valid ? t("search.for", { q }) : undefined}
      />
      {/* Phones have no top bar: the field lives here. A desktop searches from the top bar. */}
      <form
        role="search"
        className="relative md:hidden"
        onSubmit={(e) => {
          e.preventDefault()
          const next = String(new FormData(e.currentTarget).get("q") ?? "")
          router.push(`/search?${new URLSearchParams(next ? { q: next } : {})}`)
        }}
      >
        <Search
          aria-hidden="true"
          strokeWidth={1.75}
          className="pointer-events-none absolute top-1/2 left-3.5 size-[18px] -translate-y-1/2 text-muted-foreground"
        />
        <Input
          key={q}
          name="q"
          type="search"
          defaultValue={q}
          aria-label={t("shell.search")}
          placeholder={t("shell.searchPlaceholder")}
          className="pl-10"
        />
      </form>
      {!valid ? (
        <ListCard>
          <CardState>
            <Notice title={t("search.tooShort")} />
          </CardState>
        </ListCard>
      ) : results.isPending ? (
        <ListCard>
          <ListSkeleton
            rows={3}
            columns={[
              { label: t("products.name") },
              { label: t("products.number") },
              { label: t("search.ean") },
            ]}
          />
        </ListCard>
      ) : results.isError ? (
        <ListCard>
          <CardState>
            <LoadError
              error={results.error}
              onRetry={() => void results.refetch()}
            />
          </CardState>
        </ListCard>
      ) : groups.length === 0 ? (
        <ListCard>
          <CardState>
            <EmptyState title={t("search.nothing", { q })} />
          </CardState>
        </ListCard>
      ) : (
        groups
      )}
    </>
  )
}
