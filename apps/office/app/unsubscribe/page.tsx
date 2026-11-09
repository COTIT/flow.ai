"use client"

import * as React from "react"
import Link from "next/link"
import { useSearchParams } from "next/navigation"
import { useTranslations } from "next-intl"

import { useScheduledReportsUnsubscribe } from "@genesy/api-client/endpoints/scheduled-reports"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Logo } from "@genesy/ui/components/logo"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { LocaleMenu } from "@/components/shell"
import { LoadError } from "@/components/states"

/*
 * The link from a scheduled report mail. Stopping takes a click (POST), never the page load:
 * mail scanners open links, and must not stop anyone's report. The token stays in this
 * page's address only.
 */

/** Stop one scheduled report from its mail link (Template Auth, as AcceptInvitation). */
export default function UnsubscribePage() {
  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 p-6 max-md:p-4">
      <div className="flex w-[min(440px,100%)] flex-col gap-5 rounded-[20px] glass-strong p-8 max-md:p-6">
        <Logo className="h-auto w-[168px]" />
        <div className="gold-rule w-12" />
        {/* useSearchParams needs a boundary for the static build. */}
        <React.Suspense fallback={<Skeleton className="h-24 w-full" />}>
          <Stop />
        </React.Suspense>
      </div>
      <LocaleMenu withName />
    </main>
  )
}

function Stop() {
  const t = useTranslations()
  const token = useSearchParams().get("token") ?? ""
  const stop = useScheduledReportsUnsubscribe()
  const manage = (
    <Link
      href="/settings/scheduled-reports"
      className={buttonVariants({ variant: "outline" })}
    >
      {t("unsubscribe.manage")}
    </Link>
  )

  if (!token)
    return <Notice variant="error" title={t("unsubscribe.incomplete")} />
  if (stop.isSuccess)
    return (
      <>
        <Notice
          variant="success"
          title={t("unsubscribe.done", {
            report: t(`scheduled.reports.${stop.data.data.report}`),
          })}
        />
        {manage}
      </>
    )
  // A refused link (404/422) stays refused; a lost connection gets a retry.
  if (stop.isError && stop.error.response)
    return (
      <>
        <Notice variant="error" title={t("unsubscribe.invalid")} />
        {manage}
      </>
    )
  return (
    <>
      <div>
        <h1 className="text-[22px] leading-[1.3] font-medium">
          {t("unsubscribe.title")}
        </h1>
        <p className="text-[13px] text-muted-foreground">
          {t("unsubscribe.text")}
        </p>
      </div>
      {stop.isError ? (
        <LoadError
          error={stop.error}
          onRetry={() => stop.mutate({ data: { token } })}
        />
      ) : (
        <Button
          disabled={stop.isPending}
          onClick={() => stop.mutate({ data: { token } })}
        >
          {t("unsubscribe.confirm")}
        </Button>
      )}
    </>
  )
}
