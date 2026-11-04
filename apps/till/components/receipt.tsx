"use client"

import { Printer } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"

import type { SaleResource } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { estimate, type CartLine } from "@/lib/cart-store"
import { useTill } from "@/lib/till-store"
import { useTillMoney } from "@/lib/use-money"

/** "2 × Wildflower honey 500 g" rows of the basket, for a sale kept on the device. */
function Lines({
  lines,
  amount,
}: {
  lines: CartLine[]
  amount: (l: CartLine) => string
}) {
  return lines.map((l) => (
    <div
      key={`${l.productId}-${l.variantId}`}
      className="flex justify-between gap-3 text-[13px]"
    >
      <span>
        {l.quantity} × {l.name}
      </span>
      <span className="tabular-nums">{amount(l)}</span>
    </div>
  ))
}

/** The booked sale as the API answered it (TillReceipt), its booked lines included. */
export function Receipt({
  sale,
  onDone,
}: {
  sale: SaleResource
  onDone: () => void
}) {
  const t = useTranslations("receipt")
  const locale = useLocale()
  const { money } = useTillMoney()
  const session = useTill((s) => s.session)
  const cash = sale.method === "cash"

  const rows: [string, string][] = [
    ...(sale.rounding
      ? ([
          [t("rounding"), money(sale.rounding)],
          [t("due"), money(sale.due)],
        ] as [string, string][])
      : []),
    cash
      ? [t("tendered"), money(sale.tendered)]
      : [t("card"), sale.card_reference ?? "—"],
  ]

  return (
    <>
      <Notice
        variant="success"
        className="print:hidden"
        title={t("booked", { number: sale.number })}
      >
        {new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(new Date(sale.sold_at))}
        {session ? ` · ${session.register.name} · ${session.user.name}` : null}
      </Notice>
      {sale.price_mismatch ? (
        <Notice
          variant="warning"
          className="print:hidden"
          title={t("mismatch")}
        />
      ) : null}
      <div className="till-tile flex flex-col gap-2">
        {sale.lines.map((l, i) => (
          <div key={i} className="flex justify-between gap-3 text-[13px]">
            <span>
              {/* The API's decimal string ("2.00") as the basket counts it ("2"). */}
              {Number(l.quantity)} × {l.title}
            </span>
            <span className="tabular-nums">{money(l.line_total)}</span>
          </div>
        ))}
        <hr className="border-0 border-t border-[var(--hair)]" />
        <dl className="flex flex-col gap-2">
          <div className="flex justify-between gap-3 font-semibold">
            <dt>{t("total")}</dt>
            <dd className="tabular-nums">{money(sale.total)}</dd>
          </div>
          {rows.map(([label, value]) => (
            <div key={label} className="flex justify-between gap-3 text-[13px]">
              <dt className="text-muted-foreground">{label}</dt>
              <dd className="tabular-nums">{value}</dd>
            </div>
          ))}
        </dl>
      </div>
      {cash ? (
        <div className="teal-tile py-3.5">
          <span id="give-label" className="text-lg font-semibold">
            {t("giveChange")}
          </span>
          <output aria-labelledby="give-label" className="grand text-[40px]">
            {money(sale.change)}
          </output>
        </div>
      ) : null}
      <div className="print-hide mt-auto flex gap-2.5">
        <Button
          variant="outline"
          className="btn-76 w-[150px]"
          onClick={() => window.print()}
        >
          <Printer aria-hidden />
          {t("print")}
        </Button>
        <button
          type="button"
          className="charge grow justify-center"
          onClick={onDone}
        >
          {t("newSale")}
        </button>
      </div>
    </>
  )
}

/** A sale kept on the device because the API couldn't be reached. */
export function Saved({
  lines,
  onDone,
}: {
  lines: CartLine[]
  onDone: () => void
}) {
  const t = useTranslations("sale")
  const { money } = useTillMoney()
  return (
    <>
      <Notice variant="warning" title={t("savedHint")} />
      <div className="till-tile flex flex-col gap-2">
        <Lines lines={lines} amount={() => ""} />
        <hr className="border-0 border-t border-[var(--hair)]" />
        <div className="flex justify-between gap-3 font-semibold">
          <span>{t("estimate")}</span>
          <span className="tabular-nums">≈ {money(estimate(lines))}</span>
        </div>
      </div>
      <button
        type="button"
        className="charge mt-auto justify-center"
        onClick={onDone}
      >
        {t("newSale")}
      </button>
    </>
  )
}
