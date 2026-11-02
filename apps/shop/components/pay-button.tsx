"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

import { payOrder, type PayState } from "@/app/actions"

export function PayButton({ orderId }: { orderId: string }) {
  const t = useTranslations("order")
  const [state, action, pending] = React.useActionState<PayState, FormData>(
    payOrder,
    {}
  )
  const failed =
    state.problem === "provider_down" || state.problem === "unavailable"
  return (
    <form action={action} className="flex flex-col items-start gap-3">
      <input type="hidden" name="order" value={orderId} />
      {state.problem ? (
        <Notice
          // Not started for an outside reason: a calm warning. Failed: an error to retry.
          variant={failed ? "error" : "warning"}
          title={t(`problem.${state.problem}.title`)}
        >
          {t(`problem.${state.problem}.body`)}
        </Notice>
      ) : null}
      <Button
        type="submit"
        size="lg"
        disabled={pending || state.problem === "no_provider"}
      >
        {failed ? t("retry") : t("pay")}
      </Button>
    </form>
  )
}
