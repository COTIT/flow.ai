"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { ScreenError } from "@/components/screen-error"
import { isLoadFailure } from "@/lib/load-failure"

/** Inside the scenes segment: a scene that fails to draw never reaches the shell or the voice session. */
export default function SceneError({ error, reset }: { error: Error; reset: () => void }) {
  const t = useTranslations("scenes")
  // A scene whose code never arrived is loaded again for real; reset() would repeat the failure.
  if (isLoadFailure(error)) return <ScreenError error={error} reset={reset} />
  return (
    <Notice
      variant="error"
      title={t("sceneError")}
      action={
        <Button variant="outline" size="sm" onClick={reset}>
          {t("retry")}
        </Button>
      }
    />
  )
}
