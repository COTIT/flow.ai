"use client"

import * as React from "react"
import { useParams } from "next/navigation"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@verzo/api-client/auth"
import {
  getImportsGetQueryKey,
  getImportsRowsQueryKey,
  importsApply,
  useImportsGet,
  useImportsRows,
} from "@verzo/api-client/endpoints/imports"
import { ApiError } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { EmptyState } from "@verzo/ui/components/empty-state"
import { Notice } from "@verzo/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@verzo/ui/components/table"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
  Segmented,
} from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { LoadError } from "@/components/states"
import { useRights } from "@/lib/can"
import {
  nativeRight,
  TONES,
  type Batch,
  type BatchRow,
} from "@/lib/imports/flow"

type Filter = "all" | "valid" | "invalid" | "applied" | "failed"
const FILTERS: Filter[] = ["all", "valid", "invalid", "applied", "failed"]

/** Review the checked rows, import the ready ones, then read the results. */
export default function ImportPage() {
  const t = useTranslations()
  const { id } = useParams<{ id: string }>()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const [filter, setFilter] = React.useState<Filter>("all")
  const [page, setPage] = React.useState(1)
  const [applying, setApplying] = React.useState(false)
  const [error, setError] = React.useState<unknown>()
  const [stale, setStale] = React.useState(false)
  const busy = (q: { state: { data?: unknown } }) =>
    (q.state.data as { data?: Batch } | undefined)?.data?.state === "applying"
      ? 1500
      : false
  const got = useImportsGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId), refetchInterval: busy },
  })
  const batch = got.data?.data as Batch | undefined
  const params = {
    page: String(page),
    page_size: "50",
    ...(filter === "all" ? {} : { status: filter }),
  }
  const rows = useImportsRows(workspaceId, id, params, {
    query: {
      enabled: Boolean(workspaceId),
      refetchInterval: batch?.state === "applying" ? 1500 : false,
    },
  })
  const { may } = useRights()
  const mayApply = may("import.apply") && (!batch || may(nativeRight(batch)))
  const canApply = Boolean(batch?.allowed_actions.includes("apply")) && mayApply

  async function apply() {
    if (!batch) return
    setError(undefined)
    setStale(false)
    setApplying(true)
    try {
      await importsApply(
        workspaceId,
        batch.id,
        {
          client_ref: crypto.randomUUID(),
          expected_version: batch.version,
          payload: {
            validation_fingerprint: batch.validation_fingerprint ?? "",
          },
        },
        { headers: { "Idempotency-Key": crypto.randomUUID() } }
      )
    } catch (e) {
      if ((e as { response?: { status?: number } }).response?.status === 409)
        setStale(true)
      else setError(e)
    } finally {
      setApplying(false)
      await queryClient.invalidateQueries({
        queryKey: getImportsGetQueryKey(workspaceId, id),
      })
      await queryClient.invalidateQueries({
        queryKey: getImportsRowsQueryKey(workspaceId, id),
      })
    }
  }

  const columns = [
    { label: t("imports.row") },
    { label: t("imports.values") },
    { label: t("imports.state") },
    { label: t("imports.issue") },
  ]
  const list = (rows.data?.data ?? []) as BatchRow[]
  const c = batch?.counts

  return (
    <>
      <PageHead
        eyebrow={t("imports.title")}
        title={batch?.filename ?? t("imports.title")}
        back={{ href: "/settings/imports", label: t("imports.back") }}
        description={
          batch
            ? `${t(`imports.profileShort.${batch.profile}`)} · ${t("imports.counts", batch.counts)}`
            : undefined
        }
        actions={
          canApply && c ? (
            <Button disabled={applying} onClick={() => void apply()}>
              {t("imports.apply", { count: c.valid })}
            </Button>
          ) : null
        }
      />
      {got.isError ? (
        <LoadError error={got.error} onRetry={() => void got.refetch()} />
      ) : null}
      <div role="status" className="flex flex-col gap-3 empty:hidden">
        {batch?.state === "applying" ? (
          <Notice live={false} title={t("imports.applying")} />
        ) : null}
        {batch?.state === "completed" ? (
          <Notice
            live={false}
            variant="success"
            title={t("imports.completed", { count: c?.applied ?? 0 })}
          />
        ) : null}
        {batch?.state === "failed" ? (
          <Notice live={false} variant="error" title={t("imports.failed")} />
        ) : null}
        {batch?.state === "uploading" ? (
          <Notice
            live={false}
            variant="warning"
            title={t("imports.incomplete")}
          />
        ) : null}
        {batch?.state === "validated" && c?.valid === 0 ? (
          <Notice live={false} variant="warning" title={t("imports.noValid")} />
        ) : null}
        {c &&
        c.invalid > 0 &&
        batch?.state !== "uploading" &&
        !(batch?.state === "validated" && c.valid === 0) ? (
          <Notice
            live={false}
            variant="warning"
            title={t("imports.invalidNote", { count: c.invalid })}
          />
        ) : null}
        {c && c.failed > 0 ? (
          <Notice
            live={false}
            variant="warning"
            title={t("imports.rowsFailed", { count: c.failed })}
          />
        ) : null}
        {stale ? (
          <Notice live={false} variant="warning" title={t("imports.stale")} />
        ) : null}
      </div>
      <ApiError error={error} fallback={t("common.error")} />
      <ListCard
        filters={
          <Segmented
            label={t("imports.rowsTitle")}
            value={filter}
            options={FILTERS.map((f) => ({
              value: f,
              label: t(`imports.rowStates.${f}`),
            }))}
            onChange={(f) => {
              setFilter(f)
              setPage(1)
            }}
          />
        }
        pager={
          rows.data && rows.data.meta.last_page > 1 ? (
            <ListPager meta={rows.data.meta} onPage={setPage} />
          ) : undefined
        }
      >
        {rows.isPending ? (
          <ListSkeleton columns={columns} />
        ) : rows.isError ? (
          <CardState>
            <LoadError error={rows.error} onRetry={() => void rows.refetch()} />
          </CardState>
        ) : list.length === 0 ? (
          <CardState>
            <EmptyState title={t("imports.emptyRows")} />
          </CardState>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                {columns.map((col) => (
                  <TableHead key={col.label}>{col.label}</TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {list.map((r) => (
                <TableRow key={r.id}>
                  <TableCell className="font-semibold tabular-nums">
                    {t("imports.row")} {r.row_number}
                  </TableCell>
                  <TableCell data-m="sub" className="break-words">
                    {[
                      r.external_key,
                      ...Object.entries(r.fields).map(
                        ([k, v]) => `${t(`imports.fields.${k}`)}: ${v}`
                      ),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={r.status}
                      tone={TONES[r.status]}
                      label={t(`imports.rowStates.${r.status}`)}
                    />
                  </TableCell>
                  <TableCell data-m="sub" className="break-words">
                    {r.issues.map((i) => i.message).join(" ")}
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
