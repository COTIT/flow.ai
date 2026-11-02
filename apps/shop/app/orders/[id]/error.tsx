"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

/** The order page failed to load (API trouble). No detail is shown; it is logged on the server. */
export default function OrderError({ reset }: { reset: () => void }) {
  const t = useTranslations("order")
  return (
    <Notice
      variant="error"
      title={t("loadFailed.title")}
      action={<Button onClick={() => reset()}>{t("retry")}</Button>}
    >
      {t("loadFailed.body")}
    </Notice>
  )
}
