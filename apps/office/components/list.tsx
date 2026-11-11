"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"
import { ArrowLeft } from "lucide-react"

import { Card } from "@genesy/ui/components/card"
import { Pager } from "@genesy/ui/components/pager"
import { Skeleton } from "@genesy/ui/components/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { cn } from "@genesy/ui/lib/utils"

/**
 * Title row of a page (OfficeProducts `.page-head`): small caps eyebrow, 26 px title, actions
 * on the right; `back` puts a link above the title on detail pages, `tabs` a sub-menu under it.
 */
export function PageHead({
  eyebrow,
  title,
  description,
  back,
  actions,
  tabs,
  className,
}: {
  eyebrow?: React.ReactNode
  title: React.ReactNode
  description?: React.ReactNode
  back?: { href: string; label: string }
  actions?: React.ReactNode
  tabs?: React.ReactNode
  className?: string
}) {
  return (
    <div className={cn("flex flex-col gap-4", className)}>
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          {back ? (
            <Link
              href={back.href}
              className="flex items-center gap-1 text-[13px] text-muted-foreground hover:text-foreground max-md:min-h-11"
            >
              <ArrowLeft aria-hidden="true" className="size-4" strokeWidth={1.75} />
              {back.label}
            </Link>
          ) : null}
          {eyebrow ? <Eyebrow>{eyebrow}</Eyebrow> : null}
          <h1 className="text-[26px] leading-[1.3] font-medium break-words max-md:text-[22px]">
            {title}
          </h1>
          {description ? (
            <p className="text-[13px] text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {actions ? (
          <div className="flex flex-wrap items-center gap-3">{actions}</div>
        ) : null}
      </div>
      {tabs}
    </div>
  )
}

/** `.h3`: 12 px bold caps in the muted colour. */
export function Eyebrow({ children }: { children: React.ReactNode }) {
  return (
    <p className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase">
      {children}
    </p>
  )
}

// `.seg`: the pill group, and one pill; `pillOn` marks the chosen one.
const pills =
  "inline-flex max-w-full flex-wrap gap-1 self-start rounded-[14px] border border-(--hair) bg-(--glass-subtle) p-1"
const pill =
  "inline-flex h-[34px] items-center rounded-[10px] border border-transparent px-3.5 text-[13px] font-medium text-muted-foreground outline-none focus-visible:ring-3 focus-visible:ring-ring max-md:h-11"
const pillOn =
  "border-(--glass-border) bg-(--glass-strong) text-foreground shadow-[var(--glass-highlight),0_2px_8px_-4px_rgb(28_27_24/25%)]"

/**
 * Pill links (`.seg` / `.tabs`): one of a few views, each its own address. On a phone each
 * pill is a 44 px target.
 */
export function PillLinks({
  label,
  links,
}: {
  label: string
  links: { href: string; label: string; active: boolean }[]
}) {
  return (
    <nav aria-label={label} className={pills}>
      {links.map((l) => (
        <Link
          key={l.href}
          href={l.href}
          aria-current={l.active ? "page" : undefined}
          className={cn(pill, l.active && pillOn)}
        >
          {l.label}
        </Link>
      ))}
    </nav>
  )
}

/** The same pills as buttons, for a filter inside a card (OfficeProduct "Stock movements"). */
export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: T
  options: { value: T; label: string }[]
  onChange: (value: T) => void
}) {
  return (
    <div role="group" aria-label={label} className={pills}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          aria-pressed={o.value === value}
          className={cn(pill, o.value === value && pillOn)}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}

/**
 * The one glass card of a list page (OfficeProducts `.card.flush`): filters and a count in its
 * head, then the table (or its loading, empty or error state), then the pager. At 375 px the
 * rows turn into cards (office.css, `[data-list]`).
 */
export function ListCard({
  filters,
  count,
  children,
  pager,
  className,
}: {
  filters?: React.ReactNode
  count?: React.ReactNode
  children: React.ReactNode
  pager?: React.ReactNode
  className?: string
}) {
  return (
    <Card data-list="" className={cn("gap-0 py-0", className)}>
      {filters || count ? (
        <div className="flex flex-wrap items-center justify-between gap-3 px-6 pt-5 pb-4 max-md:px-4 max-md:pt-4 max-md:pb-3">
          <div className="flex min-w-0 flex-1 flex-wrap items-center gap-3">
            {filters}
          </div>
          {count ? (
            <p className="text-[13px] text-muted-foreground">{count}</p>
          ) : null}
        </div>
      ) : null}
      {children}
      {pager ? (
        <div className="border-t border-(--hair) px-5 py-3.5 max-md:px-4 max-md:py-3">
          {pager}
        </div>
      ) : null}
    </Card>
  )
}

/** A state (empty, error, notice) inside a list card, padded like the card head. */
export function CardState({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-3 px-6 pb-6 first:pt-6 max-md:px-4 max-md:pb-4 max-md:first:pt-4">
      {children}
    </div>
  )
}

/**
 * Loading rows in the table's final shape (OfficeStates "Loading · list"): the real header,
 * then bars where the cells will be.
 */
export function ListSkeleton({
  columns,
  rows = 5,
  rowHeight = "h-5",
}: {
  columns: { label: string; right?: boolean }[]
  rows?: number
  /** Height of a real row's content (a text line; `h-10` with a thumbnail), so nothing moves when data arrives. */
  rowHeight?: string
}) {
  const widths = ["w-36", "w-16", "w-24", "w-14", "w-20", "w-12"]
  return (
    <Table aria-busy="true">
      <TableHeader>
        <TableRow>
          {columns.map((c) => (
            <TableHead key={c.label} className={c.right ? "text-right" : undefined}>
              {c.label}
            </TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody>
        {Array.from({ length: rows }, (_, r) => (
          <TableRow key={r}>
            {columns.map((c, i) => (
              <TableCell key={c.label} data-m={i === 0 ? undefined : "hide"}>
                <div className={cn("flex items-center", rowHeight)}>
                  <Skeleton
                    className={cn("h-3.5", widths[(i + r) % widths.length], c.right && "ml-auto")}
                  />
                </div>
              </TableCell>
            ))}
          </TableRow>
        ))}
      </TableBody>
    </Table>
  )
}

/** The list's pager from an API `meta`, with the shared words. */
export function ListPager({
  meta,
  onPage,
}: {
  meta: { current_page: number; last_page: number; from?: number | null; to?: number | null; total: number }
  onPage: (page: number) => void
}) {
  const t = useTranslations()
  return (
    <Pager
      page={meta.current_page}
      lastPage={meta.last_page}
      summary={t("pager.summary", {
        from: meta.from ?? 0,
        to: meta.to ?? 0,
        total: meta.total,
      })}
      previous={t("pager.previous")}
      next={t("pager.next")}
      onPage={onPage}
    />
  )
}
