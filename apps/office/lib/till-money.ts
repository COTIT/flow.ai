"use client"

import { useLocale } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useCurrenciesList } from "@genesy/api-client/endpoints/currencies"
import { useTillRegistersList } from "@genesy/api-client/endpoints/till"
import { formatMoney } from "@genesy/ui/lib/money"

/**
 * Till amounts come in minor units with only a currency or register id: this looks up the
 * currency's ISO code and decimals to format them. "—" until the lookups are in.
 */
export function useTillMoney() {
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const locale = useLocale()
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  // shortcut: first 100 currencies and registers; page these when a workspace has more.
  const currencies = useCurrenciesList(workspaceId, { page_size: 100 }, enabled)
  const registers = useTillRegistersList(
    workspaceId,
    { page_size: 100 },
    enabled
  )
  const register = (id: string) => registers.data?.data.find((r) => r.id === id)
  const money = (
    minor: number | null | undefined,
    currencyId: string | undefined
  ) => {
    const c = currencies.data?.data.find((x) => x.id === currencyId)
    return minor == null || !c
      ? "—"
      : formatMoney(minor, c.decimals, c.asset_type, locale)
  }
  return {
    currencies: currencies.data?.data ?? [],
    registers: registers.data?.data ?? [],
    register,
    money,
    /** An amount of a register's own currency (shifts, sales). */
    registerMoney: (minor: number | null | undefined, registerId: string) =>
      money(minor, register(registerId)?.currency_id),
  }
}
