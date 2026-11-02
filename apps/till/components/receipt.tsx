"use client"

import { useLocale, useTranslations } from "next-intl"

import type { SaleResource } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@/components/state"
import type { CartLine } from "@/lib/cart-store"
import { useTill } from "@/lib/till-store"
import { useTillMoney } from "@/lib/use-money"

/** The booked sale as the API answered it, with the lines as rung up. */
export function Receipt({
  sale,
  lines,
  onDone,
}: {
  sale: SaleResource
  lines: CartLine[]
  onDone: () => void
}) {
  const t = useTranslations("receipt")
  const locale = useLocale()
  const { money } = useTillMoney()
  const session = useTill((s) => s.session)

  const rows: [string, string][] = [
    [t("total"), money(sale.total)],
    ...(sale.rounding
      ? [[t("rounding"), money(sale.rounding)] as [string, string]]
      : []),
    [t("due"), money(sale.due)],
    ...(sale.method === "cash"
      ? [
          [t("tendered"), money(sale.tendered)] as [string, string],
          [t("change"), money(sale.change)] as [string, string],
        ]
      : [[t("card"), sale.card_reference ?? "—"] as [string, string]]),
  ]

  return (
    <section
      aria-label={t("title")}
      className="flex max-w-md flex-col gap-3 rounded-lg border p-4"
    >
      <h1 className="text-xl font-medium">
        {t("title")} {sale.number}
      </h1>
      <Notice kind="success" className="print:hidden">
        {t("booked", { number: sale.number })}
      </Notice>
      {sale.price_mismatch ? (
        <Notice kind="warning" className="print:hidden">
          {t("mismatch")}
        </Notice>
      ) : null}
      <p className="text-sm text-muted-foreground">
        {new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(new Date(sale.sold_at))}
        {session ? ` · ${session.register.name} · ${session.user.name}` : null}
      </p>
      <ul className="flex flex-col gap-1 text-sm">
        {lines.map((l) => (
          <li key={`${l.productId}-${l.variantId}`} className="flex gap-2">
            <span className="mr-auto">
              {l.quantity} × {l.name}
            </span>
          </li>
        ))}
      </ul>
      <dl className="grid grid-cols-2 gap-1 border-t pt-2 text-sm">
        {rows.map(([label, value]) => (
          <div key={label} className="contents">
            <dt>{label}</dt>
            <dd className="text-right tabular-nums">{value}</dd>
          </div>
        ))}
      </dl>
      <div className="flex gap-2 print:hidden">
        <Button variant="outline" onClick={() => window.print()}>
          {t("print")}
        </Button>
        <Button className="flex-1" onClick={onDone}>
          {t("newSale")}
        </Button>
      </div>
    </section>
  )
}
