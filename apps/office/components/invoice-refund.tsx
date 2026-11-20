"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useMutation } from "@tanstack/react-query"
import { useSession } from "@verzo/api-client/auth"
import { apiRequest } from "@verzo/api-client/mutator"
import type { InvoiceResource } from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@verzo/ui/components/dialog"
import { Input } from "@verzo/ui/components/input"
import { Notice } from "@verzo/ui/components/notice"
import { Field } from "@/components/field"
import { Done, useDone } from "@/components/states"
import { useCan } from "@/lib/can"
import { useIdempotencyKey } from "@/lib/idempotency"

/** Credit note refunds use the local payment UUID; the server checks source and exact balance. */
export function InvoiceRefund({
  invoice,
  onChanged,
}: {
  invoice: InvoiceResource
  onChanged: () => void
}) {
  const t = useTranslations()
  const mayRefund = useCan("invoice.credit")
  const workspace = useSession((s) => s.workspaceId)
  const [open, setOpen] = React.useState(false)
  const key = useIdempotencyKey()
  const done = useDone()
  const refund = useMutation({
    mutationFn: (data: { payment_id: string; amount: string }) =>
      apiRequest(
        {
          url: `/workspace/${workspace}/invoices/${invoice.id}/refunds`,
          method: "POST",
          data,
        },
        key.request
      ),
    onMutate: done.clear,
    onSuccess: () => {
      setOpen(false)
      done.show(t("payments.refundSubmitted"))
      onChanged()
    },
    onSettled: (_data, error) => key.settle(error),
  })
  if (
    !invoice.is_credit ||
    !mayRefund ||
    !["pending", "partially_credited"].includes(invoice.status)
  )
    return null
  const errors = fieldErrors(refund.error)
  return (
    <>
      <Done message={done.message} />
      <Button
        variant="outline"
        className="w-full"
        onClick={() => {
          refund.reset()
          setOpen(true)
        }}
      >
        {t("payments.refund")}
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("payments.refund")}</DialogTitle>
          </DialogHeader>
          <form
            className="flex flex-col gap-4"
            onSubmit={(event) => {
              event.preventDefault()
              const form = new FormData(event.currentTarget)
              refund.mutate({
                payment_id: String(form.get("payment_id")).trim(),
                amount: String(form.get("amount")).trim(),
              })
            }}
          >
            <Notice variant="info" title={t("payments.refundHint")} />
            <Field
              id="refund-source"
              label={t("payments.sourcePayment")}
              errors={errors.payment_id}
            >
              <Input
                name="payment_id"
                required
                pattern="[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
              />
            </Field>
            <Field
              id="refund-amount"
              label={t("invoices.payAmount", {
                currency: invoice.money.currency ?? "",
              })}
              errors={errors.amount}
            >
              <Input name="amount" inputMode="decimal" required />
            </Field>
            <p className="text-sm text-muted-foreground">
              {t("payments.exactAmount")}
            </p>
            <ApiError error={refund.error} fallback={t("common.error")} />
            <Button type="submit" disabled={refund.isPending}>
              {t("payments.refund")}
            </Button>
          </form>
        </DialogContent>
      </Dialog>
    </>
  )
}
