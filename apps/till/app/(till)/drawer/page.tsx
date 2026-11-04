"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import {
  getTillShiftCurrentQueryKey,
  useTillShiftClose,
  useTillShiftCurrent,
  useTillShiftOpen,
} from "@genesy/api-client/endpoints/till"
import type { ShiftResource } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { cn } from "@genesy/ui/lib/utils"
import { CountForm } from "@/components/count-form"
import { useLock } from "@/components/till-top"
import { pendingCount, useQueue } from "@/lib/sale-queue"
import { useTill } from "@/lib/till-store"
import { useTillMoney } from "@/lib/use-money"
import { refusal } from "@/lib/refusal"

/** The register's drawer (TillDrawer): open a shift with a counted float, close it with a count. */
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
  const [opened, setOpened] = React.useState(false)
  const { lock, pending: locking } = useLock()

  const current = useTillShiftCurrent(workspace)
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getTillShiftCurrentQueryKey(workspace),
    })
  const open = useTillShiftOpen({
    mutation: {
      onSuccess: () => {
        setOpened(true)
        void refresh()
      },
    },
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
  const tr = useTranslations("refusals")
  const refused = (e: unknown) => {
    const r = refusal(e, tr, t("refused"))
    return (
      <Notice variant="error" title={r.title}>
        {r.detail}
      </Notice>
    )
  }
  const difference = closed?.difference ?? 0
  const time = (iso: string) =>
    new Intl.DateTimeFormat(locale, { timeStyle: "short" }).format(
      new Date(iso)
    )

  if (closed)
    return (
      <main className="till-center">
        <section className="till-card strong w-[min(620px,100%)] gap-[18px] p-7 max-[760px]:p-4">
          <Notice
            variant={difference === 0 ? "success" : "warning"}
            title={
              difference === 0
                ? t("balanced")
                : difference < 0
                  ? t("short", { amount: money(-difference) })
                  : t("over", { amount: money(difference) })
            }
          >
            {t("closedBody")}
          </Notice>
          <div className="tile3">
            {(
              [
                ["expected", closed.expected],
                ["counted", closed.counted],
                ["difference", closed.difference],
              ] as const
            ).map(([key, value]) => (
              <div
                key={key}
                className={cn(
                  "till-tile",
                  key === "difference" &&
                    "border-transparent " +
                      (difference === 0
                        ? "bg-[var(--c-teal)] text-[var(--t-teal)]"
                        : "bg-[var(--c-gold)] text-[var(--t-gold)]")
                )}
              >
                <p
                  id={`result-${key}`}
                  className="text-xs text-muted-foreground"
                >
                  {t(key)}
                </p>
                <output
                  aria-labelledby={`result-${key}`}
                  className="till-h2 block tabular-nums"
                >
                  {money(value)}
                </output>
              </div>
            ))}
          </div>
          <div className="flex gap-2.5">
            <Button
              variant="outline"
              className="btn-64 flex-1 text-base"
              disabled={locking}
              onClick={lock}
            >
              {t("lock")}
            </Button>
            <Button
              className="btn-64 flex-1 text-base"
              onClick={() => {
                setClosed(null)
                setOpened(false)
              }}
            >
              {t("openNew")}
            </Button>
          </div>
        </section>
      </main>
    )

  if (current.error)
    return (
      <main className="till-center">
        <Notice variant="error" title={t("loadFailed")} />
      </main>
    )

  if (current.isPending)
    return (
      <main className="split split-wide" aria-busy aria-label={tc("loading")}>
        <div className="till-card gap-3">
          {Array.from({ length: 6 }, (_, i) => (
            <Skeleton key={i} className="h-11 w-full" />
          ))}
        </div>
        <div className="till-card strong tight">
          <Skeleton className="h-10 w-full" />
        </div>
      </main>
    )

  return shift ? (
    <CountForm
      key={`close-${cash}`}
      title={t("closeTitle")}
      sub={t("openedWith", {
        time: time(shift.opened_at),
        amount: money(shift.opening),
      })}
      badge={<span className="tbadge teal">{t("shiftOpen")}</span>}
      expected={money(shift.expected)}
      aside={
        <>
          {opened ? <Notice variant="success" title={t("opened")} /> : null}
          {waiting ? (
            <Notice
              variant="warning"
              title={t("queued", { count: waiting })}
              action={
                <Link
                  href="/queue"
                  prefetch={false}
                  className="badge-link underline"
                >
                  {t("toQueue")}
                </Link>
              }
            />
          ) : null}
          {close.error ? (
            refused(close.error)
          ) : null}
        </>
      }
      submitLabel={t("close")}
      pending={close.isPending || waiting > 0}
      onSubmit={(count) => close.mutate({ workspace, data: { count } })}
    />
  ) : (
    <CountForm
      key={`open-${cash}`}
      title={t("openTitle")}
      sub={t("floatHint")}
      badge={<span className="tbadge slate">{t("noShift")}</span>}
      aside={
        <>
          <Notice variant="info" title={t("none")} />
          {open.error ? (
            refused(open.error)
          ) : null}
        </>
      }
      submitLabel={t("open")}
      pending={open.isPending}
      onSubmit={(count) => {
        setOpened(false)
        open.mutate({ workspace, data: { count } })
      }}
    />
  )
}
