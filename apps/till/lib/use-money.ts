"use client"

import { useLocale } from "next-intl"

import { formatMoney } from "@genesy/ui/lib/money"
import { useTill } from "@/lib/till-store"

/** Formats minor units in the register's currency (from the login); "—" until there is one. */
export function useTillMoney() {
  const locale = useLocale()
  const currency = useTill((s) => s.session?.register.currency)
  return {
    decimals: currency?.decimals ?? null,
    money: (minor: number | null | undefined) =>
      minor == null || !currency
        ? "—"
        : formatMoney(minor, currency.decimals, currency.code, locale),
  }
}
