"use client"

import { useQuery } from "@tanstack/react-query"
import { useLocale } from "next-intl"

import { storefrontProductsList } from "@genesy/api-client/endpoints/storefront"
import { formatMoney } from "@genesy/ui/lib/money"
import { deviceAuth, useTill } from "@/lib/till-store"

/**
 * Formats the register's minor units; "—" until the currency is known.
 * shortcut: currency from one storefront row (device key) until `/till/me` carries the register's.
 */
export function useTillMoney() {
  const locale = useLocale()
  const device = useTill((s) => s.device)
  const { data } = useQuery({
    queryKey: ["till-currency", device?.workspaceId],
    queryFn: () => storefrontProductsList({ page_size: 1 }, deviceAuth(device)),
    enabled: Boolean(device),
    staleTime: Infinity,
  })
  const first = data?.data[0]
  const currency = first
    ? { code: first.currency, decimals: first.currency_decimals }
    : null
  return {
    decimals: currency?.decimals ?? null,
    money: (minor: number | null | undefined) =>
      minor == null || !currency
        ? "—"
        : formatMoney(minor, currency.decimals, currency.code, locale),
  }
}
