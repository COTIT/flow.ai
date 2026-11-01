"use client"

import { useLocale } from "next-intl"

import { formatMoney } from "@genesy/ui/lib/money"
import { useTillCatalog } from "@/lib/use-catalog"

/** Formats the register's minor units; "—" until the currency is known. */
export function useTillMoney() {
  const locale = useLocale()
  const { currency } = useTillCatalog()
  return {
    decimals: currency?.decimals ?? null,
    money: (minor: number | null | undefined) =>
      minor == null || !currency
        ? "—"
        : formatMoney(minor, currency.decimals, currency.code, locale),
  }
}
