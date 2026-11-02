"use client"

import Link from "next/link"
import { useTranslations } from "next-intl"

import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

/** Any screen that crashed. No detail is shown; queued sales stay on the device. */
export default function ErrorPage({ reset }: { reset: () => void }) {
  const t = useTranslations("errors")
  return (
    <main className="flex min-h-svh flex-col items-start gap-4 p-6">
      <Notice variant="error" title={t("crashTitle")}>
        {t("crashBody")}
      </Notice>
      <div className="flex gap-2">
        <Button onClick={() => reset()}>{t("retry")}</Button>
        <Link href="/sale" className={buttonVariants({ variant: "outline" })}>
          {t("home")}
        </Link>
      </div>
    </main>
  )
}
