"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

import { card } from "@/lib/look"

/**
 * Mock `ShopStates` "Shop unavailable": any failed page, e.g. the storefront API down. Shows
 * no error detail; it is logged on the server.
 */
export default function ErrorPage({ reset }: { reset: () => void }) {
  const t = useTranslations("errors")
  return (
    <div className={card}>
      <Notice
        variant="error"
        title={t("unavailableTitle")}
        action={
          <Button variant="outline" size="sm" onClick={() => reset()}>
            {t("retry")}
          </Button>
        }
      >
        {t("unavailableBody")}
      </Notice>
    </div>
  )
}
