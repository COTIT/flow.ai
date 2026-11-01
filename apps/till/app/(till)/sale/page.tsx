"use client"

import * as React from "react"
import Link from "next/link"
import { Minus, Plus, X } from "lucide-react"
import { useTranslations } from "next-intl"

import { useTillSalesCreate } from "@genesy/api-client/endpoints/till"
import type { SaleResource } from "@genesy/api-client/model"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"
import { toMinor } from "@genesy/ui/lib/money"
import { Receipt } from "@/components/receipt"
import {
  cartStore,
  estimate,
  toSaleLines,
  useCart,
  type CartLine,
} from "@/lib/cart-store"
import { useTill } from "@/lib/till-store"
import { useTillCatalog, type Sellable } from "@/lib/use-catalog"
import { useTillMoney } from "@/lib/use-money"

/** Ring up a sale: search or scan, adjust the cart, take cash or card, show the receipt. */
export default function SalePage() {
  const t = useTranslations("sale")
  const tc = useTranslations("common")
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  const catalog = useTillCatalog()
  const { money, decimals } = useTillMoney()
  const lines = useCart((s) => s.lines)
  const [query, setQuery] = React.useState("")
  const [code, setCode] = React.useState("")
  const [notFound, setNotFound] = React.useState<string | null>(null)
  const [method, setMethod] = React.useState<"cash" | "card">("cash")
  const [tendered, setTendered] = React.useState("")
  const [reference, setReference] = React.useState("")
  const [badTendered, setBadTendered] = React.useState(false)
  const [done, setDone] = React.useState<{
    sale: SaleResource
    lines: CartLine[]
  } | null>(null)

  const sale = useTillSalesCreate({
    mutation: {
      onSuccess: ({ data }) => {
        setDone({ sale: data, lines })
        cartStore.getState().clear()
        setTendered("")
        setReference("")
      },
    },
  })

  const add = (item: Sellable) => {
    sale.reset()
    cartStore.getState().add(item)
  }

  if (done)
    return (
      <Receipt
        sale={done.sale}
        lines={done.lines}
        onDone={() => setDone(null)}
      />
    )

  const results = catalog.search(query).slice(0, 20)
  const status = sale.error?.response?.status
  const fieldErrors = sale.error?.response?.data
    ? Object.values(
        (sale.error.response.data as { errors?: Record<string, string[]> })
          .errors ?? {}
      ).flat()
    : []

  return (
    <div className="grid gap-6 md:grid-cols-2">
      <section className="flex flex-col gap-3">
        <h1 className="text-xl font-medium">{t("title")}</h1>
        <form
          className="flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault()
            const item = catalog.findByCode(code)
            setNotFound(item ? null : code)
            if (item) add(item)
            setCode("")
          }}
        >
          <Label htmlFor="sale-code">{t("scan")}</Label>
          <Input
            id="sale-code"
            value={code}
            autoComplete="off"
            onChange={(e) => setCode(e.target.value)}
          />
          {notFound != null ? (
            <p role="alert" className="text-sm text-destructive">
              {t("notFound", { code: notFound })}
            </p>
          ) : null}
        </form>
        <div className="flex flex-col gap-2">
          <Label htmlFor="sale-search">{t("search")}</Label>
          <Input
            id="sale-search"
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <ApiError error={catalog.error} fallback={tc("error")} />
          <ul aria-label={t("results")} className="flex flex-col gap-1">
            {results.map((item) => (
              <li key={`${item.productId}-${item.variantId}`}>
                <Button
                  variant="outline"
                  className="h-auto w-full justify-between py-2 text-left whitespace-normal"
                  onClick={() => add(item)}
                >
                  <span>{item.name}</span>
                  <span className="tabular-nums">{money(item.unitGross)}</span>
                </Button>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section aria-label={t("cart")} className="flex flex-col gap-3">
        <h2 className="font-medium">{t("cart")}</h2>
        {lines.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("empty")}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {lines.map((l, i) => (
              <li
                key={`${l.productId}-${l.variantId}`}
                className="flex flex-wrap items-center gap-2 text-sm"
              >
                <span className="mr-auto min-w-0">
                  {l.name}
                  <span className="block text-muted-foreground tabular-nums">
                    {money(l.unitGross)}
                  </span>
                </span>
                <Button
                  variant="outline"
                  size="icon"
                  aria-label={t("less", { name: l.name })}
                  onClick={() =>
                    l.quantity > 1
                      ? cartStore.getState().setQuantity(i, l.quantity - 1)
                      : cartStore.getState().remove(i)
                  }
                >
                  <Minus />
                </Button>
                <span
                  aria-label={t("quantityOf", { name: l.name })}
                  className="w-8 text-center tabular-nums"
                >
                  {l.quantity}
                </span>
                <Button
                  variant="outline"
                  size="icon"
                  aria-label={t("more", { name: l.name })}
                  onClick={() =>
                    cartStore.getState().setQuantity(i, l.quantity + 1)
                  }
                >
                  <Plus />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("remove", { name: l.name })}
                  onClick={() => cartStore.getState().remove(i)}
                >
                  <X />
                </Button>
              </li>
            ))}
          </ul>
        )}
        <p className="flex justify-between border-t pt-2 font-medium">
          <span>{t("estimate")}</span>
          <span className="tabular-nums">≈ {money(estimate(lines))}</span>
        </p>
        <p className="text-xs text-muted-foreground">{t("estimateHint")}</p>

        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const cash =
              method === "cash" && decimals != null
                ? toMinor(tendered, decimals)
                : null
            setBadTendered(method === "cash" && cash == null)
            if (method === "cash" && cash == null) return
            sale.mutate({
              workspace,
              data: {
                client_ref: cartStore.getState().startCharge(),
                lines: toSaleLines(lines),
                payment:
                  method === "cash"
                    ? { method: "cash", tendered: cash }
                    : { method: "card", reference: reference.trim() || null },
              },
            })
          }}
        >
          <fieldset className="flex gap-2">
            <legend className="sr-only">{t("method")}</legend>
            {(["cash", "card"] as const).map((m) => (
              <Button
                key={m}
                type="button"
                variant={method === m ? "default" : "outline"}
                aria-pressed={method === m}
                className="flex-1"
                onClick={() => setMethod(m)}
              >
                {t(m)}
              </Button>
            ))}
          </fieldset>
          {method === "cash" ? (
            <div className="flex flex-col gap-2">
              <Label htmlFor="sale-tendered">{t("tendered")}</Label>
              <Input
                id="sale-tendered"
                inputMode="decimal"
                value={tendered}
                aria-invalid={badTendered || undefined}
                onChange={(e) => setTendered(e.target.value)}
              />
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <Label htmlFor="sale-reference">{t("reference")}</Label>
              <Input
                id="sale-reference"
                maxLength={64}
                value={reference}
                onChange={(e) => setReference(e.target.value)}
              />
            </div>
          )}
          {badTendered ? (
            <p role="alert" className="text-sm text-destructive">
              {t("badTendered")}
            </p>
          ) : null}
          {sale.error ? (
            <div role="alert" className="flex flex-col gap-1 text-sm text-destructive">
              <p>
                {(sale.error.response?.data as { message?: string } | undefined)
                  ?.message ?? tc("error")}
              </p>
              {fieldErrors.map((m) => (
                <p key={m}>{m}</p>
              ))}
              {status === 409 ? (
                <Link href="/drawer" className="underline">
                  {t("toDrawer")}
                </Link>
              ) : null}
            </div>
          ) : null}
          <Button
            type="submit"
            size="lg"
            disabled={lines.length === 0 || sale.isPending}
          >
            {t("charge")}
          </Button>
        </form>
      </section>
    </div>
  )
}
