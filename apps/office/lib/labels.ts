// The API sends these as plain strings; the lists name every code office shows as a word.
export const INVOICE_STATUSES = [
  "draft",
  "sent",
  "open",
  "partially_paid",
  "paid",
  "cancelled",
  "credited",
] as const
export const PAY_METHODS = [
  "cash",
  "card",
  "pin",
  "manual",
  "provider",
  "bank",
  "bank_transfer",
  "mollie",
  "online",
] as const
export const MOVEMENT_TYPES = [
  "increase",
  "decrease",
  "set_level",
  "set_requirements",
] as const
export const LOT_EVENT_KINDS = [
  "created",
  "updated",
  "status_changed",
  "passport_published",
  "passport_unpublished",
  "packed",
  "dispatched",
] as const

/** A code as a message key: `catalog:read` → `catalog_read`, `stock.changed` → `stock_changed`. */
export const labelKey = (code: string) => code.replace(/[:.]/g, "_")
