"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

/** A screen that crashed while rendering: say so and offer to try again, never a blank page. */
export default function ErrorPage({
  reset,
}: {
  error: Error
  reset: () => void
}) {
  const t = useTranslations("states")
  return (
    <main className="mx-auto flex min-h-svh max-w-lg flex-col justify-center p-4">
      <Notice
        variant="error"
        title={t("crashed")}
        action={
          <Button variant="outline" size="sm" onClick={reset}>
            {t("retry")}
          </Button>
        }
      >
        {t("crashedText")}
      </Notice>
    </main>
  )
}
