"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

/** Inside the scenes segment: a scene that fails to draw never reaches the shell or the voice session. */
export default function SceneError({ reset }: { reset: () => void }) {
  const t = useTranslations("scenes")
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
