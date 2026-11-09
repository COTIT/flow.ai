"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getScheduledReportsListQueryKey,
  useScheduledReportsDelete,
  useScheduledReportsList,
  useScheduledReportsUpdate,
} from "@genesy/api-client/endpoints/scheduled-reports"
import type { ScheduledReport } from "@genesy/api-client/model"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { formatDateTime } from "@genesy/ui/lib/format"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { timingWords } from "@/lib/scheduled-reports"

/** The caller's own scheduled reports (made by voice): pause, resume, delete. */
export default function ScheduledReportsPage() {
  const t = useTranslations()
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const list = useScheduledReportsList(workspaceId, {
    query: { enabled: Boolean(workspaceId) },
  })
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getScheduledReportsListQueryKey(workspaceId),
    })
  const done = useDone()
  const update = useScheduledReportsUpdate({
    mutation: {
      onMutate: done.clear,
      onSuccess: (res) => {
        done.show(t(res.data.active ? "scheduled.resumed" : "scheduled.paused"))
        return refresh()
      },
    },
  })
  const remove = useScheduledReportsDelete({
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        done.show(t("scheduled.deleted"))
        return refresh()
      },
    },
  })
  // The report being confirmed; kept while the dialog closes so its title does not blank.
  const [deleting, setDeleting] = React.useState<{
    id: string
    name: string
    open: boolean
  }>()
  const rows = list.data?.data ?? []
  const name = (s: ScheduledReport) => t(`scheduled.reports.${s.report}`)
  const when = (s: ScheduledReport) => {
    const w = timingWords(s, locale)
    return t(w.key, w.values)
  }
  const columns = [
    { label: t("scheduled.report") },
    { label: t("scheduled.schedule") },
    { label: t("scheduled.nextRun") },
    { label: t("scheduled.status") },
    { label: t("scheduled.actions"), right: true },
  ]
  const busy = update.isPending || remove.isPending

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("scheduled.title")}
        description={
          rows[0]
            ? `${t("scheduled.intro")} ${t("scheduled.zone", { zone: rows[0].time_zone })}`
            : t("scheduled.intro")
        }
      />
      <Done message={done.message} />
      <ApiError
        error={update.error ?? remove.error}
        fallback={t("common.error")}
      />
      <ListCard>
        {list.isPending ? (
          <ListSkeleton columns={columns} />
        ) : list.isError ? (
          <CardState>
            <LoadError error={list.error} onRetry={() => void list.refetch()} />
          </CardState>
        ) : rows.length === 0 ? (
          <CardState>
            <EmptyState
              title={t("scheduled.empty")}
              description={t("scheduled.emptyHint")}
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
              {rows.map((s) => (
                <TableRow key={s.id}>
                  <TableCell className="font-semibold">{name(s)}</TableCell>
                  <TableCell data-m="sub">{when(s)}</TableCell>
                  <TableCell
                    data-m="hide"
                    className="text-muted-foreground tabular-nums"
                  >
                    {s.active ? formatDateTime(s.next_run_at, locale) : "—"}
                  </TableCell>
                  <TableCell data-m="hide">
                    <span
                      title={
                        s.paused_reason === "access_lost"
                          ? t("scheduled.lostHint")
                          : undefined
                      }
                    >
                      <StatusBadge
                        status={s.active ? "active" : "paused"}
                        tone={s.active ? "teal" : "slate"}
                        label={t(
                          `scheduled.states.${s.paused_reason ?? "active"}`
                        )}
                      />
                    </span>
                  </TableCell>
                  <TableCell data-m="badge" className="text-right">
                    <span className="inline-flex gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={busy}
                        aria-label={`${t(s.active ? "scheduled.pause" : "scheduled.resume")}: ${name(s)}`}
                        onClick={() =>
                          update.mutate({
                            workspace: workspaceId,
                            schedule: s.id,
                            data: { active: !s.active },
                          })
                        }
                      >
                        {t(s.active ? "scheduled.pause" : "scheduled.resume")}
                      </Button>
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={busy}
                        aria-label={`${t("scheduled.delete")}: ${name(s)}`}
                        onClick={() =>
                          setDeleting({ id: s.id, name: name(s), open: true })
                        }
                      >
                        {t("scheduled.delete")}
                      </Button>
                    </span>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
      <ConfirmDialog
        open={deleting?.open ?? false}
        onOpenChange={(open) => setDeleting((d) => d && { ...d, open })}
        title={t("scheduled.deleteTitle", { report: deleting?.name ?? "" })}
        description={t("scheduled.deleteText")}
        confirm={t("scheduled.delete")}
        cancel={t("actions.cancel")}
        disabled={remove.isPending}
        onConfirm={() =>
          deleting &&
          remove.mutate({ workspace: workspaceId, schedule: deleting.id })
        }
      />
    </>
  )
}
