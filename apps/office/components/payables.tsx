"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import type { JournalPreview } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@genesy/ui/components/select"
import { Input } from "@genesy/ui/components/input"
import { Skeleton } from "@genesy/ui/components/skeleton"
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { useRouter } from "next/navigation"

import { useSession } from "@genesy/api-client/auth"
import { useRelationsList } from "@genesy/api-client/endpoints/relations"
import { Field } from "@/components/field"
import { CardState, ListCard, ListPager, ListSkeleton } from "@/components/list"
import { Empty, LoadError } from "@/components/states"
import { useIdempotencyKey } from "@/lib/idempotency"

import { bookAmount, isStale } from "@/app/(app)/purchases/shared"
import type { Outcome } from "@/app/(app)/purchases/[id]/dialogs"

/** A label for an API word under `payables.<group>`, or the word itself when unknown. */
export function usePayWord() {
  const t = useTranslations("payables")
  return (group: string, word: string) =>
    t.has(`${group}.${word}`) ? t(`${group}.${word}`) : word
}

/** Every refusal code the payables API sends, as the sentence staff read (never the code). */
export function usePayReasons() {
  const t = useTranslations("payables.reasons")
  return Object.fromEntries(
    [
      "stale_document",
      "dependent_effects_exist",
      "accounting_period_closed",
      "accounting_not_configured",
      "unsupported_active_erp_operation",
    ].map((c) => [c, t(c)])
  )
}

/** OfficeOrder's label/value row. */
export function Fact({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <p className="flex items-center justify-between gap-3">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right break-words">{value}</span>
    </p>
  )
}

/** OfficeOrder's small-caps side card ("Next step", "Supplier"). */
export function SideCard({
  id,
  title,
  children,
}: {
  id: string
  title: string
  children: React.ReactNode
}) {
  return (
    <Card
      role="region"
      aria-labelledby={id}
      className="col-span-4 gap-4 px-6 py-6 max-md:col-span-12 max-md:gap-3 max-md:p-4"
    >
      <h2 id={id} className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase">
        {title}
      </h2>
      {children}
    </Card>
  )
}

/** The server's journal preview (functional amounts as sent; never summed here). */
export function JournalList({ journal }: { journal?: JournalPreview[] | null }) {
  const t = useTranslations()
  const locale = useLocale()
  if (!journal?.length) return null
  const role = (r?: string | null) =>
    r && t.has(`accounting.roles.${r}`) ? t(`accounting.roles.${r}`) : t("payables.account")
  return (
    <div className="flex flex-col gap-1">
      <h4 className="text-xs font-semibold text-muted-foreground">{t("payables.journal")}</h4>
      <ul className="flex flex-col gap-1">
        {journal.map((j, i) => (
          <li key={i} className="flex justify-between gap-3">
            <span>{role(j.role)}</span>
            <span className="tabular-nums">
              {Number(j.debit) ? `${t("payables.debit")} ${bookAmount(j.debit, locale)}` : null}
              {Number(j.credit) ? `${t("payables.credit")} ${bookAmount(j.credit, locale)}` : null}
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}

type Reviewed = { review_fingerprint: string; journal?: JournalPreview[] | null }

/**
 * One reviewed action (post, reverse, allocate, record): the server's review first (no writes),
 * then the action with its fingerprint. With `auto`, the review runs when the dialog opens;
 * otherwise the inputs in `fields` are filled first. After a review the inputs lock; "Change"
 * drops the review. A 409 stale closes the dialog and reloads the page's record.
 */
export function ReviewDialog<R extends Reviewed>({
  button,
  trigger,
  title,
  description,
  outcome,
  done,
  auto = false,
  ready = true,
  confirmLabel,
  fields,
  onOpen,
  review,
  renderReview,
  commit,
}: {
  button: React.ReactElement
  trigger: string
  title: string
  description: string
  outcome: Outcome
  done: string
  auto?: boolean
  ready?: boolean
  confirmLabel?: string
  fields?: (errors: Record<string, string[]>) => React.ReactNode
  onOpen?: () => void
  review: () => Promise<R>
  renderReview: (r: R) => React.ReactNode
  commit: (
    fingerprint: string,
    clientRef: string,
    request: { headers: Record<string, string> }
  ) => Promise<unknown>
}) {
  const t = useTranslations()
  const reasons = usePayReasons()
  const key = useIdempotencyKey()
  const [open, setOpen] = React.useState(false)
  const [r, setR] = React.useState<R | null>(null)
  const [error, setError] = React.useState<unknown>(null)
  const [busy, setBusy] = React.useState(false)
  const runReview = async () => {
    setBusy(true)
    setError(null)
    try {
      setR(await review())
    } catch (e) {
      setError(e)
    } finally {
      setBusy(false)
    }
  }
  const runCommit = async () => {
    if (!r) return
    setBusy(true)
    setError(null)
    try {
      await commit(r.review_fingerprint, key.request.headers["Idempotency-Key"], key.request)
      key.settle(null)
      setOpen(false)
      outcome.done(done)
    } catch (e) {
      key.settle(e)
      if (isStale(e)) {
        setOpen(false)
        outcome.stale()
      } else setError(e)
    } finally {
      setBusy(false)
    }
  }
  const errors = fieldErrors(error)
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (o) {
          key.renew()
          setR(null)
          setError(null)
          onOpen?.()
          if (auto) void runReview()
        }
      }}
    >
      <DialogTrigger render={button}>{trigger}</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        {fields ? (
          <fieldset className="flex flex-col gap-3" disabled={Boolean(r) || busy}>
            {fields(errors)}
          </fieldset>
        ) : null}
        {busy && !r ? (
          <div aria-busy="true" className="flex flex-col gap-2">
            <span className="sr-only">{t("payables.reviewing")}</span>
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-4 w-1/2" />
          </div>
        ) : null}
        {r ? (
          <section aria-label={t("payables.reviewTitle")} className="flex flex-col gap-2 text-[13px]">
            <h3 className="text-sm font-semibold">{t("payables.reviewTitle")}</h3>
            {renderReview(r)}
            <JournalList journal={r.journal} />
          </section>
        ) : null}
        <ApiError error={error} fallback={t("common.error")} codes={reasons} />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          {r ? (
            <>
              {!auto ? (
                <Button type="button" variant="outline" onClick={() => setR(null)}>
                  {t("payables.change")}
                </Button>
              ) : null}
              <Button disabled={busy} onClick={() => void runCommit()}>
                {confirmLabel ?? trigger}
              </Button>
            </>
          ) : auto ? (
            error ? (
              <Button disabled={busy} onClick={() => void runReview()}>
                {t("payables.reviewAgain")}
              </Button>
            ) : null
          ) : (
            <Button disabled={!ready || busy} onClick={() => void runReview()}>
              {t("payables.review")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

type Page<T> = {
  data: T[]
  meta: { current_page: number; last_page: number; from?: number | null; to?: number | null; total: number }
}
type ListQuery<T> = {
  isPending: boolean
  isError: boolean
  error: unknown
  data?: Page<T>
  refetch: () => unknown
}

/**
 * A payables list (Template List, as /purchases): supplier filter, count, server rows, pager,
 * and every state (loading, error, empty, filtered-empty).
 */
export function PayablesList<T extends { id: string }>({
  supplier,
  query,
  columns,
  row,
  count,
  empty,
}: {
  supplier?: string
  query: ListQuery<T>
  columns: { label: string; right?: boolean }[]
  row: (item: T) => React.ReactNode
  count: (total: number) => string
  empty: string
}) {
  const t = useTranslations()
  const router = useRouter()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const suppliers = useRelationsList(
    workspaceId,
    { suppliers: true, limit: 100 },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const go = (next: { supplier?: string; page?: number }) => {
    const params = new URLSearchParams()
    const s = next.supplier ?? supplier
    if (s) params.set("supplier", s)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }
  const items = [
    { value: "", label: t("payables.allSuppliers") },
    ...(suppliers.data?.data ?? []).map((s) => ({
      value: s.id,
      label: s.company_name ?? s.alias ?? s.relation_nr ?? s.id,
    })),
  ]
  return (
    <ListCard
      filters={
        <Select items={items} value={supplier ?? ""} onValueChange={(v) => go({ supplier: v ?? "" })}>
          <SelectTrigger aria-label={t("payables.supplier")} className="w-[240px] max-md:w-full">
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
      }
      count={query.data ? count(query.data.meta.total) : null}
      pager={query.data?.data.length ? <ListPager meta={query.data.meta} onPage={(p) => go({ page: p })} /> : null}
    >
      {query.isPending ? (
        <ListSkeleton columns={columns} />
      ) : query.isError ? (
        <CardState>
          <LoadError error={query.error} onRetry={() => void query.refetch()} />
        </CardState>
      ) : !query.data?.data.length ? (
        <CardState>
          <Empty title={empty} onClear={supplier ? () => router.push("?") : undefined} />
        </CardState>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              {columns.map((c) => (
                <TableHead key={c.label} className={c.right ? "text-right" : undefined}>
                  {c.label}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>{query.data.data.map((item) => row(item))}</TableBody>
        </Table>
      )}
    </ListCard>
  )
}

/** Accounting date and reason for a reverse dialog; `reset` on open. */
export function useReverseInputs(prefix: string) {
  const t = useTranslations()
  const [date, setDate] = React.useState("")
  const [reason, setReason] = React.useState("")
  return {
    body: { accounting_date: date, reason: reason.trim() },
    ready: Boolean(date && reason.trim()),
    reset: (today: string) => {
      setDate(today)
      setReason("")
    },
    fields: (errors: Record<string, string[]>) => (
      <>
        <Field
          id={`${prefix}-date`}
          label={t("payables.accountingDate")}
          errors={errors["accounting_date"] ?? errors["payload.accounting_date"]}
        >
          <Input type="date" required value={date} onChange={(e) => setDate(e.target.value)} />
        </Field>
        <Field
          id={`${prefix}-reason`}
          label={t("payables.reason")}
          errors={errors["reason"] ?? errors["payload.reason"]}
        >
          <Input required maxLength={1000} value={reason} onChange={(e) => setReason(e.target.value)} />
        </Field>
      </>
    ),
  }
}
