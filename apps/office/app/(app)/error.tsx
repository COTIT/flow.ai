"use client"

import { useTranslations } from "next-intl"

import { Button } from "@verzo/ui/components/button"
import { Card, CardContent } from "@verzo/ui/components/card"
import { Notice } from "@verzo/ui/components/notice"

/** A signed-in screen that crashed while rendering: the shell stays, the card offers a retry. */
export default function AppErrorPage({
  reset,
}: {
  error: Error
  reset: () => void
}) {
  const t = useTranslations("states")
  return (
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
  )
}
