const variants = {
  queued: "info",
  accepted: "info",
  manual: "info",
  blocked: "warning",
  failed: "warning",
  unknown: "warning",
} as const

/** A successful write can number an invoice without delivering its email. */
export function invoiceDeliveryNotice(body: unknown, invoiceId: string) {
  const uncertain = {
    key: "invoices.delivery.uncertain",
    variant: "warning",
  } as const
  if (
    !body ||
    typeof body !== "object" ||
    !("message" in body) ||
    body.message !== "ok" ||
    !("delivery" in body)
  )
    return uncertain
  const delivery = body.delivery
  if (
    !delivery ||
    typeof delivery !== "object" ||
    !("invoice_id" in delivery) ||
    delivery.invoice_id !== invoiceId ||
    !("status" in delivery) ||
    typeof delivery.status !== "string" ||
    !Object.hasOwn(variants, delivery.status)
  )
    return uncertain
  const status = delivery.status as keyof typeof variants
  return {
    key: `invoices.delivery.${status}`,
    variant: variants[status],
  } as const
}
