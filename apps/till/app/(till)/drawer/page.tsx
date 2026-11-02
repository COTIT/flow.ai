"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import {
  getTillShiftCurrentQueryKey,
  useTillShiftClose,
  useTillShiftCurrent,
  useTillShiftOpen,
} from "@genesy/api-client/endpoints/till"
import type { ShiftResource } from "@genesy/api-client/model"
import { ApiError } from "@genesy/ui/components/api-error"
import { Badge } from "@genesy/ui/components/badge"
import { CountForm } from "@/components/count-form"
import { pendingCount, useQueue } from "@/lib/sale-queue"
import { useTill } from "@/lib/till-store"
import { useTillMoney } from "@/lib/use-money"

/** The register's drawer: open a shift with a counted float, see it, close it with a count. */
export default function DrawerPage() {
  const t = useTranslations("drawer")
  const tc = useTranslations("common")
  const locale = useLocale()
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  // Rebuild the count rows when /till/me brings another list of notes and coins.
  const cash = useTill((s) => s.session?.register.accepted_cash?.join())
  const queryClient = useQueryClient()
  const { money } = useTillMoney()
  // A queued cash sale lands in the shift open when it replays: sync before closing.
  const waiting = useQueue(pendingCount)
  const [closed, setClosed] = React.useState<ShiftResource | null>(null)

  const current = useTillShiftCurrent(workspace)
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getTillShiftCurrentQueryKey(workspace),
    })
  const open = useTillShiftOpen({
    mutation: { onSuccess: () => refresh() },
  })
  const close = useTillShiftClose({
    mutation: {
      onSuccess: ({ data }) => {
        setClosed(data)
        void refresh()
      },
    },
  })

  const shift = current.data?.data
  const time = (iso: string) =>
    new Intl.DateTimeFormat(locale, {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(iso))

  return (
    <section className="flex max-w-xl flex-col gap-4">
      <h1 className="text-xl font-medium">{t("title")}</h1>
      <ApiError error={current.error} fallback={tc("error")} />

      {closed ? (
        <div className="flex flex-col gap-2 rounded-lg border p-4">
          <h2 className="font-medium">{t("closedTitle")}</h2>
          <dl className="grid grid-cols-2 gap-1 text-sm">
            <dt>{t("expected")}</dt>
            <dd className="tabular-nums">{money(closed.expected)}</dd>
            <dt>{t("counted")}</dt>
            <dd className="tabular-nums">{money(closed.counted)}</dd>
            <dt>{t("difference")}</dt>
            <dd className="flex items-center gap-2 tabular-nums">
              {money(closed.difference)}
              <Badge
                variant={closed.difference === 0 ? "secondary" : "destructive"}
              >
                {closed.difference === 0
                  ? t("balanced")
                  : (closed.difference ?? 0) < 0
                    ? t("short")
                    : t("over")}
              </Badge>
            </dd>
          </dl>
        </div>
      ) : null}

      {current.isPending ? null : shift ? (
        <>
          <dl className="grid grid-cols-2 gap-1 text-sm">
            <dt>{t("openedAt")}</dt>
            <dd>{time(shift.opened_at)}</dd>
            <dt>{t("opening")}</dt>
            <dd className="tabular-nums">{money(shift.opening)}</dd>
            <dt>{t("expected")}</dt>
            <dd className="font-medium tabular-nums">
              {money(shift.expected)}
            </dd>
          </dl>
          <h2 className="font-medium">{t("closeTitle")}</h2>
          {waiting ? (
            <p role="alert" className="text-sm text-destructive">
              {t("queued", { count: waiting })}
            </p>
          ) : null}
          <CountForm
            key={cash}
            submitLabel={t("close")}
            pending={close.isPending || waiting > 0}
            onSubmit={(count) => close.mutate({ workspace, data: { count } })}
          />
          <ApiError error={close.error} fallback={tc("error")} />
        </>
      ) : current.isSuccess ? (
        <>
          <p className="text-sm text-muted-foreground">{t("none")}</p>
          <h2 className="font-medium">{t("openTitle")}</h2>
          <CountForm
            key={cash}
            submitLabel={t("open")}
            pending={open.isPending}
            onSubmit={(count) => {
              setClosed(null)
              open.mutate({ workspace, data: { count } })
            }}
          />
          <ApiError error={open.error} fallback={tc("error")} />
        </>
      ) : null}
    </section>
  )
}
