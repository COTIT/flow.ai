"use client"

import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import {
  getScheduledReportsListQueryKey,
  useScheduledReportsDelete,
  useScheduledReportsUpdate,
} from "@genesy/api-client/endpoints/scheduled-reports"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { Notice } from "@genesy/ui/components/notice"
import { formatDateTime } from "@genesy/ui/lib/format"
import { useReports, useWorkspace } from "@/lib/use-scene-data"
import { useSyncGuard } from "@/lib/use-sync-guard"
import { DataState, Restricted } from "./data-state"
import type { RegistryProps } from "./types"

/** My Routines: the caller's own scheduled reports. Creating one stays a reviewed voice action. */
export function ReportList({ restricted }: RegistryProps) {
  const t = useTranslations("scenes")
  if (restricted) return <Restricted title={t("reports")} />
  return <Reports />
}

function Reports() {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const ws = useWorkspace()
  const client = useQueryClient()
  const query = useReports()
  const done = {
    onSettled: () =>
      client.invalidateQueries({
        queryKey: getScheduledReportsListQueryKey(ws),
      }),
  }
  const update = useScheduledReportsUpdate({ mutation: done })
  const remove = useScheduledReportsDelete({ mutation: done })
  const once = useSyncGuard()
  const rows = query.data?.data ?? []
  return (
    <section
      aria-label={t("reports")}
      className="flex flex-col gap-3 rounded-2xl border bg-card p-3"
    >
      <h3 className="px-1 text-sm font-semibold">{t("reports")}</h3>
      {update.isError || remove.isError ? (
        <Notice variant="error" title={t("saveFailed")} />
      ) : null}
      <DataState query={query} empty={!rows.length} emptyTitle={t("noReports")}>
        <ul className="flex flex-col gap-2">
          {rows.map((r) => (
            <li
              key={r.id}
              data-report={r.id}
              className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-xl px-2 py-2"
            >
              <div className="mr-auto flex min-w-0 flex-col">
                <span className="font-medium">
                  {t(`report.${r.report}`)} · {t(`frequency.${r.frequency}`)}
                </span>
                <span className="text-xs text-muted-foreground">
                  {r.active ? t("active") : t("paused")}
                  {r.active
                    ? ` · ${t("nextRun", { when: formatDateTime(r.next_run_at, locale) })}`
                    : null}
                </span>
              </div>
              <Button
                variant="outline"
                size="sm"
                className="min-h-11"
                disabled={update.isPending}
                onClick={() =>
                  void once(() =>
                    update
                      .mutateAsync({
                        workspace: ws,
                        schedule: r.id,
                        data: { active: !r.active },
                      })
                      .catch(() => null)
                  )
                }
              >
                {r.active ? t("pause") : t("resume")}
              </Button>
              <ConfirmDialog
                trigger={t("delete")}
                title={t("deleteTitle")}
                description={t("deleteText")}
                confirm={t("delete")}
                cancel={t("cancel")}
                destructive
                disabled={remove.isPending}
                onConfirm={() =>
                  void once(() =>
                    remove
                      .mutateAsync({ workspace: ws, schedule: r.id })
                      .catch(() => null)
                  )
                }
              />
            </li>
          ))}
        </ul>
      </DataState>
      <p className="px-1 text-xs text-muted-foreground">{t("planned")}</p>
    </section>
  )
}
