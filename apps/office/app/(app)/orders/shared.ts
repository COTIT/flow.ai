// The statuses the API documents for an order, for the filter and the labels.
export const orderStatuses = [
  "draft",
  "awaiting_payment",
  "open",
  "completed",
  "partially_returned",
  "returned",
  "cancelled",
] as const

/** The web customer's name, else e-mail; office orders have none. */
export function customerName(customer: object | null): string {
  const c = customer as { name?: string | null; email?: string | null } | null
  return c?.name || c?.email || ""
}
