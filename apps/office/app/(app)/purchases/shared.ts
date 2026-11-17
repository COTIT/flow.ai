"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useTranslations } from "next-intl"

import {
  getPurchaseOrdersEventsQueryKey,
  getPurchaseOrdersGetQueryKey,
  getPurchaseOrdersListQueryKey,
} from "@genesy/api-client/endpoints/purchase-orders"
import {
  getPurchaseReceiptsGetQueryKey,
  getPurchaseReceiptsListQueryKey,
} from "@genesy/api-client/endpoints/purchase-receipts"
import type { CurrencyRef } from "@genesy/api-client/model"

import { decimalAmount } from "@/lib/payments"

export const poStates = ["draft", "confirmed", "cancelled"] as const

/** The API's money string shown as is (no maths): "EUR 12.50". */
export const showMoney = (amount: string, currency: CurrencyRef, locale: string) =>
  `${currency.code} ${decimalAmount(amount, currency.decimals, locale)}`

/** An API amount whose currency the response doesn't name, shown with its own decimals. */
export const bookAmount = (amount: string, locale: string) =>
  decimalAmount(amount, amount.split(".")[1]?.length ?? 0, locale)

/** An exchange rate as the API sent it, without trailing zeros (Intl formats the string). */
export const showRate = (rate: string, locale: string) =>
  new Intl.NumberFormat(locale, { maximumFractionDigits: 8 }).format(rate as unknown as number)

/** A label for an API word under `purchases.<group>`, or the word itself when unknown. */
export function useWord() {
  const t = useTranslations("purchases")
  return (group: string, word: string) =>
    t.has(`${group}.${word}`) ? t(`${group}.${word}`) : word
}

/** Every refusal code the purchasing API sends, as the sentence staff read (never the code). */
export function useReasons() {
  const t = useTranslations("purchases.reasons")
  return Object.fromEntries(
    [
      "stale_document",
      "receipt_booking_required",
      "backdated_valuation",
      "accounting_period_closed",
      "accounting_not_configured",
      "unsupported_active_erp_operation",
    ].map((c) => [c, t(c)])
  )
}

/** True for the 409 the API sends when the order or receipt changed since it was read. */
export const isStale = (error: unknown) =>
  (error as { response?: { status?: number; data?: { code?: string } } } | null)
    ?.response?.data?.code === "stale_document"

/** Reload everything a purchase order page shows (after a change, or a stale refusal). */
export function useRefreshOrder(workspace: string, order: string) {
  const queryClient = useQueryClient()
  return React.useCallback(
    (receipt?: string) => {
      const keys = [
        getPurchaseOrdersGetQueryKey(workspace, order),
        getPurchaseOrdersListQueryKey(workspace),
        getPurchaseOrdersEventsQueryKey(workspace, order),
        getPurchaseReceiptsListQueryKey(workspace, order),
        ...(receipt ? [getPurchaseReceiptsGetQueryKey(workspace, order, receipt)] : []),
      ]
      for (const queryKey of keys) void queryClient.invalidateQueries({ queryKey })
    },
    [queryClient, workspace, order]
  )
}

/** Today in the browser's calendar, as the API's date string. */
export const today = () => {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}
