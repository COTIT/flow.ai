"use client"

import Link from "next/link"
import { useTranslations } from "next-intl"

import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { cn } from "@genesy/ui/lib/utils"

/** Any screen that crashed (POS panel). No detail is shown; queued sales stay on the device. */
export default function ErrorPage({ reset }: { reset: () => void }) {
  const t = useTranslations("errors")
  return (
    <main className="till">
      <div className="till-center">
        <section className="till-card strong w-[min(620px,100%)] gap-[18px] p-7 max-[760px]:p-4">
          <Notice variant="error" title={t("crashTitle")}>
            {t("crashBody")}
          </Notice>
          <div className="flex gap-2.5">
            <Button
              variant="outline"
              className="btn-64 flex-1 text-base"
              onClick={() => reset()}
            >
              {t("retry")}
            </Button>
            <Link
              href="/sale"
              className={cn(buttonVariants(), "btn-64 flex-1 text-base")}
            >
              {t("home")}
            </Link>
          </div>
        </section>
      </div>
    </main>
  )
}
