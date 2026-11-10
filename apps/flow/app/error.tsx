"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Card, CardContent } from "@genesy/ui/components/card"
import { Notice } from "@genesy/ui/components/notice"

/** A screen that crashed while rendering: one card with a retry. */
export default function ErrorPage({
  reset,
}: {
  error: Error
  reset: () => void
}) {
  const t = useTranslations("flow")
  return (
    <main className="flex min-h-svh flex-col justify-center p-4">
      <Card className="mx-auto w-full max-w-lg">
        <CardContent>
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
        </CardContent>
      </Card>
    </main>
  )
}
