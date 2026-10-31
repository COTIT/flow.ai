"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"

/** Any failed page, e.g. the storefront API down. Shows no error detail; it is logged on the server. */
export default function ErrorPage({ reset }: { reset: () => void }) {
  const t = useTranslations("errors")
  return (
    <div role="alert" className="flex flex-col items-start gap-4 py-10">
      <h1 className="heading-1">{t("unavailableTitle")}</h1>
      <p className="text-muted-foreground">{t("unavailableBody")}</p>
      <Button onClick={() => reset()}>{t("retry")}</Button>
    </div>
  )
}
