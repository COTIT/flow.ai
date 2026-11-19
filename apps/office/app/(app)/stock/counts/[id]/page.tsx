"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  getStocktakesGetQueryKey,
  getStocktakesListQueryKey,
  useStocktakesCount,
  useStocktakesGet,
} from "@genesy/api-client/endpoints/stocktakes"
import { useWarehousesList } from "@genesy/api-client/endpoints/warehouses"
import type { StocktakeResource } from "@genesy/api-client/model"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
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
import { formatDateTime, formatDay, formatQty } from "@genesy/ui/lib/format"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { ListPager, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useRights } from "@/lib/can"
import { useIdempotencyKey } from "@/lib/idempotency"

import { useCountReasons, useStateBadge, type Outcome } from "../shared"
import { CancelDialog, lineLabel, PostDialog, StartDialog } from "./dialogs"

const PAGE = 50
type Entry = { counted: string; value: string }

/** One stock count (Detail tiles): lines to count, the next step for its state, its facts. */
export default function CountPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const rights = useRights()
  const badge = useStateBadge()
  const queryClient = useQueryClient()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const count = useStocktakesGet(workspaceId, id, enabled)
  // shortcut: names for the first 100 warehouses; page these when a workspace holds more.
  const warehouses = useWarehousesList(
    workspaceId,
    { page_size: 100, roots_only: false },
    enabled
  )
  const done = useDone()
  const [stale, setStale] = React.useState(false)
  const [ended, setEnded] = React.useState(false)
  const refresh = () => {
    for (const queryKey of [
      getStocktakesGetQueryKey(workspaceId, id),
      getStocktakesListQueryKey(workspaceId),
    ])
      void queryClient.invalidateQueries({ queryKey })
  }
  const outcome: Outcome = {
    done: (message) => {
      setStale(false)
      done.show(message)
      refresh()
    },
    stale: () => {
      done.clear()
      setStale(true)
      refresh()
    },
  }
  const ending: Outcome = {
    ...outcome,
    done: (message) => {
      setEnded(true)
      outcome.done(message)
    },
  }

  if (count.isPending) return <DetailSkeleton />
  if (count.isError)
    return (
      <LoadError
        error={count.error}
        onRetry={() => void count.refetch()}
        backHref="/stock/counts"
      />
    )
  const s = count.data.data
  const may = (action: string, right: string) =>
    s.allowed_actions.includes(action) && rights.may(right)
  const mayStart = may("start", "stocktake.start")
  const mayCount = may("count", "stocktake.count")
  const mayPost = may("post", "stocktake.post")
  const mayCancel = may("cancel", "stocktake.cancel")
  const warehouse =
    warehouses.data?.data.find((w) => w.id === s.warehouse_id)?.name ?? ""

  return (
    <>
      <PageHead
        back={{ href: "/stock/counts", label: t("stocktakes.title") }}
        title={
          warehouse
            ? t("stocktakes.titleOf", { warehouse })
            : t("stocktakes.one")
        }
        description={s.reason}
        actions={<StatusBadge status={s.state} {...badge(s.state)} />}
      />
      {stale ? (
        <Notice
          variant="warning"
          title={t("accounting.reasons.stale_document")}
        />
      ) : null}
      <Bento>
        <Tile
          id="st-lines"
          title={t("stocktakes.lines")}
          span="col-span-8 row-span-2 max-md:row-span-1"
          flush
        >
          <Lines
            key={`${s.id}-${s.version}`}
            stocktake={s}
            editable={mayCount}
            outcome={outcome}
          />
        </Tile>

        <SideCard id="st-next" title={t("stocktakes.nextStep")}>
          <Done message={done.message} />
          {ended ? <Notice title={t("stocktakes.reallocated")} /> : null}
          {s.state === "counting" ? (
            <Notice variant="warning" title={t("stocktakes.paused")} />
          ) : null}
          {mayStart ? (
            <>
              <p className="text-[13px]">{t("stocktakes.next.start")}</p>
              <StartDialog
                stocktake={s}
                outcome={outcome}
                button={<Button size="lg" className="w-full" />}
              />
            </>
          ) : null}
          {mayPost ? (
            <>
              <p className="text-[13px]">{t("stocktakes.next.post")}</p>
              <PostDialog
                stocktake={s}
                outcome={ending}
                button={<Button size="lg" className="w-full" />}
              />
            </>
          ) : null}
          {mayCancel ? (
            <CancelDialog
              stocktake={s}
              outcome={ending}
              button={<Button variant="outline" className="w-full" />}
            />
          ) : null}
          {!mayStart && !mayPost && !mayCancel ? (
            <Notice
              title={
                s.state === "posted"
                  ? t("stocktakes.postedInfo")
                  : s.state === "cancelled"
                    ? t("stocktakes.cancelledInfo")
                    : t("stocktakes.nothingToDo")
              }
            />
          ) : null}
        </SideCard>

        <SideCard id="st-facts" title={t("stocktakes.details")}>
          <SubTile className="flex flex-col gap-1 text-[13px]">
            <Fact
              label={t("stocktakes.created")}
              value={formatDateTime(s.created_at, locale)}
            />
            {s.started_at ? (
              <Fact
                label={t("stocktakes.startedAt")}
                value={formatDateTime(s.started_at, locale)}
              />
            ) : null}
            {s.posted_at ? (
              <Fact
                label={t("stocktakes.postedAt")}
                value={formatDateTime(s.posted_at, locale)}
              />
            ) : null}
            {s.accounting_date ? (
              <Fact
                label={t("stocktakes.post.date")}
                value={formatDay(s.accounting_date, locale)}
              />
            ) : null}
            {s.cancelled_at ? (
              <Fact
                label={t("stocktakes.cancelledAt")}
                value={formatDateTime(s.cancelled_at, locale)}
              />
            ) : null}
          </SubTile>
          {s.cancel_reason ? (
            <p className="text-[13px] break-words">{s.cancel_reason}</p>
          ) : null}
        </SideCard>
      </Bento>
    </>
  )
}

/** The frozen lines, 50 per page; while counting, an input per line and one save for all changes. */
function Lines({
  stocktake,
  editable,
  outcome,
}: {
  stocktake: StocktakeResource
  editable: boolean
  outcome: Outcome
}) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useCountReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const lines = stocktake.lines ?? []
  const [page, setPage] = React.useState(1)
  const [edits, setEdits] = React.useState<Record<string, Entry>>({})
  const key = useIdempotencyKey()
  const save = useStocktakesCount({
    mutation: {
      onSuccess: () => outcome.done(t("done.stocktakes.counted")),
      onError: (e: unknown) => {
        if ((e as { response?: { status?: number } }).response?.status === 409)
          outcome.stale()
      },
      onSettled: (_d: unknown, e: unknown) => key.settle(e),
    },
    request: key.request,
  })
  const changed = Object.entries(edits).filter(([, e]) => e.counted !== "")
  if (lines.length === 0)
    return (
      <div className="px-6 py-6 max-md:px-4">
        <Notice
          title={
            stocktake.state === "draft"
              ? t("stocktakes.linesAfterStart")
              : t("stocktakes.noLines")
          }
        />
      </div>
    )
  const last = Math.ceil(lines.length / PAGE)
  const shown = lines.slice((page - 1) * PAGE, page * PAGE)
  const entry = (l: (typeof lines)[number]): Entry =>
    edits[l.stock_identity] ?? {
      counted: l.counted_quantity ?? "",
      value: l.positive_adjustment_value ?? "",
    }
  const set = (id: string, e: Entry) => setEdits((all) => ({ ...all, [id]: e }))
  return (
    <form
      className="flex flex-col"
      onSubmit={(e) => {
        e.preventDefault()
        // shortcut: the first 1000 changes per save (the API's cap); saving again sends the rest.
        save.mutate({
          workspace: workspaceId,
          stocktake: stocktake.id,
          data: {
            client_ref: key.request.headers["Idempotency-Key"],
            expected_version: stocktake.version,
            payload: {
              lines: changed.slice(0, 1000).map(([id, e]) => ({
                stock_identity: id,
                counted_quantity: e.counted,
                positive_adjustment_value: e.value === "" ? null : e.value,
              })),
            },
          },
        })
      }}
    >
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("stocktakes.item")}</TableHead>
            <TableHead className="text-right">
              {t("stocktakes.expected")}
            </TableHead>
            <TableHead className="text-right">
              {t("stocktakes.counted")}
            </TableHead>
            {editable ? (
              <TableHead className="text-right">
                {t("stocktakes.foundValue")}
              </TableHead>
            ) : null}
          </TableRow>
        </TableHeader>
        <TableBody>
          {shown.map((l) => {
            const e = entry(l)
            const label = lineLabel(l)
            return (
              <TableRow key={l.stock_identity}>
                <TableCell className="font-medium">{label}</TableCell>
                <TableCell data-m="sub" className="text-right tabular-nums">
                  {formatQty(l.expected_quantity, locale)}
                </TableCell>
                <TableCell data-m="right" className="text-right tabular-nums">
                  {editable ? (
                    <Input
                      aria-label={t("stocktakes.countedOf", { item: label })}
                      inputMode="decimal"
                      className="ml-auto w-28 text-right"
                      value={e.counted}
                      onChange={(ev) =>
                        set(l.stock_identity, {
                          ...e,
                          counted: ev.target.value,
                        })
                      }
                    />
                  ) : l.counted_quantity != null ? (
                    formatQty(l.counted_quantity, locale)
                  ) : (
                    t("stocktakes.notCounted")
                  )}
                </TableCell>
                {editable ? (
                  <TableCell data-m="hide" className="text-right">
                    <Input
                      aria-label={t("stocktakes.foundValueOf", { item: label })}
                      inputMode="decimal"
                      className="ml-auto w-28 text-right"
                      value={e.value}
                      onChange={(ev) =>
                        set(l.stock_identity, { ...e, value: ev.target.value })
                      }
                    />
                  </TableCell>
                ) : null}
              </TableRow>
            )
          })}
        </TableBody>
      </Table>
      {last > 1 ? (
        <div className="border-t border-(--hair) px-5 py-3.5 max-md:px-4 max-md:py-3">
          <ListPager
            meta={{
              current_page: page,
              last_page: last,
              from: (page - 1) * PAGE + 1,
              to: Math.min(page * PAGE, lines.length),
              total: lines.length,
            }}
            onPage={setPage}
          />
        </div>
      ) : null}
      {editable ? (
        <div className="flex flex-col gap-3 border-t border-(--hair) px-5 py-4 max-md:px-4">
          <p className="text-xs text-muted-foreground">
            {t("stocktakes.foundValueHint")}
          </p>
          <ApiError
            error={save.error}
            fallback={t("common.error")}
            codes={reasons}
          />
          <Button
            type="submit"
            className="self-end max-md:w-full"
            disabled={changed.length === 0 || save.isPending}
          >
            {t("stocktakes.save")}
          </Button>
        </div>
      ) : null}
    </form>
  )
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <p className="flex items-center justify-between gap-3">
      <span className="text-muted-foreground">{label}</span>
      <span className="break-words">{value}</span>
    </p>
  )
}

function SideCard({
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
      <h2
        id={id}
        className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
      >
        {title}
      </h2>
      {children}
    </Card>
  )
}
