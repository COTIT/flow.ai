"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

import { OrderCard } from "@/components/order-card"

/**
 * The order page failed to load (API trouble): `ShopStates` "Shop unavailable" in the order
 * card. No detail is shown; it is logged on the server.
 */
export default function OrderError({ reset }: { reset: () => void }) {
  const t = useTranslations("order")
  return (
    <OrderCard>
      <Notice
        variant="error"
        title={t("loadFailed.title")}
        action={
          <Button variant="outline" size="sm" onClick={() => reset()}>
            {t("retry")}
          </Button>
        }
      >
        {t("loadFailed.body")}
      </Notice>
    </OrderCard>
  )
}
