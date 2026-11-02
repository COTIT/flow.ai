"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

/** Any failed page, e.g. the storefront API down. Shows no error detail; it is logged on the server. */
export default function ErrorPage({ reset }: { reset: () => void }) {
  const t = useTranslations("errors")
  return (
    <Notice
      variant="error"
      title={t("unavailableTitle")}
      action={<Button onClick={() => reset()}>{t("retry")}</Button>}
      className="my-10"
    >
      {t("unavailableBody")}
    </Notice>
  )
}
