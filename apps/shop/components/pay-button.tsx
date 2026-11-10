"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

import { payOrder, type PayState } from "@/app/actions"

/** Starts the payment. Sits inside the awaiting notice, as drawn; its own problems show above it. */
export function PayButton({
  orderId,
  label,
  pendingReview = false,
}: {
  orderId: string
  /** "Pay $48.50": the order's total, from the server. */
  label: string
  pendingReview?: boolean
}) {
  const t = useTranslations("order")
  const [state, action, pending] = React.useActionState<PayState, FormData>(
    payOrder,
    {}
  )
  const failed =
    state.problem === "provider_down" || state.problem === "unavailable"
  if (pendingReview || state.problem === "pending_review")
    return (
      <Notice
        live={false}
        variant="warning"
        title={t("problem.pending_review.title")}
      >
        {t("problem.pending_review.body")}
      </Notice>
    )
  return (
    <form action={action} className="flex flex-col items-start gap-3">
      <input type="hidden" name="order" value={orderId} />
      {state.problem ? (
        <Notice
          // Not started for an outside reason: a calm warning. Failed: an error to retry.
          variant={failed ? "error" : "warning"}
          // A warning sits in the order's own live region; an error is announced at once.
          live={failed}
          title={t(`problem.${state.problem}.title`)}
        >
          {t(`problem.${state.problem}.body`)}
        </Notice>
      ) : null}
      <Button
        type="submit"
        disabled={pending || state.problem === "no_provider"}
        size="lg"
      >
        {failed ? t("retry") : label}
      </Button>
    </form>
  )
}
