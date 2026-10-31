"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"

import { payOrder, type PayState } from "@/app/actions"

export function PayButton({ orderId }: { orderId: string }) {
  const t = useTranslations("order")
  const [state, action, pending] = React.useActionState<PayState, FormData>(
    payOrder,
    {}
  )
  return (
    <form action={action} className="flex flex-col items-start gap-3">
      <input type="hidden" name="order" value={orderId} />
      {state.problem ? (
        <div
          role={state.problem === "no_provider" ? "status" : "alert"}
          className="rounded-lg border border-gold bg-gold/15 p-4"
        >
          <p className="font-medium">{t(`problem.${state.problem}.title`)}</p>
          <p className="text-sm">{t(`problem.${state.problem}.body`)}</p>
        </div>
      ) : null}
      <Button
        type="submit"
        size="lg"
        disabled={pending || state.problem === "no_provider"}
      >
        {t("pay")}
      </Button>
    </form>
  )
}
