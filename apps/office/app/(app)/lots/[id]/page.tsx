"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getLotsEventsQueryKey,
  getLotsGetQueryKey,
  getLotsListQueryKey,
  useLotsEvents,
  useLotsGet,
  useLotsUpdate,
} from "@genesy/api-client/endpoints/lots"
import type { UpdateLotRequest } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { StatusBadge } from "@/components/status-badge"

import { LotFields } from "../lot-fields"

export default function LotPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const lot = useLotsGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId) },
  })
  const [eventsPage, setEventsPage] = React.useState(1)
  const events = useLotsEvents(
    workspaceId,
    id,
    { page: eventsPage },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const [pending, setPending] = React.useState<UpdateLotRequest | null>(null)
  const update = useLotsUpdate({
    mutation: {
      onSuccess: () => {
        for (const queryKey of [
          getLotsGetQueryKey(workspaceId, id),
          getLotsEventsQueryKey(workspaceId, id),
          getLotsListQueryKey(workspaceId),
        ])
          void queryClient.invalidateQueries({ queryKey })
      },
    },
  })
  const save = (data: UpdateLotRequest) =>
    update.mutate({ workspace: workspaceId, lot: id, data })
  const statusLabel = (s: string) =>
    t.has(`lots.statuses.${s}`) ? t(`lots.statuses.${s}`) : s

  if (lot.isPending)
    return (
      <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
    )
  if (lot.isError)
    return <ApiError error={lot.error} fallback={t("common.error")} />
  const current = lot.data.data

  return (
    <>
      <section className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <h1 className="heading-2 text-lg">{current.code}</h1>
          <StatusBadge
            status={current.status}
            label={statusLabel(current.status)}
          />
          <span className="ml-auto font-mono text-xs text-muted-foreground">
            {current.public_code}
          </span>
          <Link
            href={`/lots/${id}/passport`}
            className="text-sm font-medium underline-offset-4 hover:underline"
          >
            {t("lots.passport")}
          </Link>
        </div>
        <form
          key={current.updated_at}
          className="flex max-w-2xl flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const text = (k: string) => String(f.get(k) ?? "") || null
            const data = {
              code: text("code"),
              status: text("status"),
              produced_on: text("produced_on"),
              best_before: text("best_before"),
              expires_on: text("expires_on"),
              origin: text("origin"),
            }
            // A recall shows on every jar's public page: ask first.
            if (data.status === "recalled" && current.status !== "recalled")
              setPending(data)
            else save(data)
          }}
        >
          <LotFields lot={current} errors={fieldErrors(update.error)} />
          <ApiError error={update.error} fallback={t("common.error")} />
          {update.isSuccess && !update.isPending ? (
            <p role="status" className="text-sm text-muted-foreground">
              {t("actions.saved")}
            </p>
          ) : null}
          <Button
            type="submit"
            className="self-start"
            disabled={update.isPending}
          >
            {t("actions.save")}
          </Button>
        </form>
        <ConfirmDialog
          open={pending !== null}
          onOpenChange={(o) => !o && setPending(null)}
          title={t("lots.recallTitle")}
          description={t("lots.recallText", { code: current.code })}
          confirm={t("lots.recallConfirm")}
          cancel={t("actions.cancel")}
          onConfirm={() => pending && save(pending)}
        />
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="heading-2 text-base">{t("lots.events")}</h2>
        {events.isPending ? (
          <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
        ) : events.isError ? (
          <ApiError error={events.error} fallback={t("common.error")} />
        ) : (
          <>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("lots.eventAt")}</TableHead>
                  <TableHead>{t("lots.eventKind")}</TableHead>
                  <TableHead>{t("lots.eventChange")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {events.data.data.map((ev) => (
                  <TableRow key={ev.id}>
                    <TableCell className="tabular-nums">
                      {new Date(ev.created_at).toLocaleString()}
                    </TableCell>
                    <TableCell>{ev.kind}</TableCell>
                    <TableCell>
                      {ev.from_status || ev.to_status
                        ? `${ev.from_status ? statusLabel(ev.from_status) : "—"} → ${ev.to_status ? statusLabel(ev.to_status) : "—"}`
                        : ""}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <Pager
              page={events.data.meta.current_page}
              lastPage={events.data.meta.last_page}
              summary={t("pager.summary", {
                from: events.data.meta.from ?? 0,
                to: events.data.meta.to ?? 0,
                total: events.data.meta.total,
              })}
              previous={t("pager.previous")}
              next={t("pager.next")}
              onPage={setEventsPage}
            />
          </>
        )}
      </section>
    </>
  )
}
