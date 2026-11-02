"use client"

import * as React from "react"
import Link from "next/link"
import { Banknote, CreditCard, Minus, Plus, ScanBarcode, X } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  useTillSalesCreate,
  useTillShiftCurrent,
} from "@genesy/api-client/endpoints/till"
import type {
  QuoteLine,
  SaleRequest,
  SaleResource,
} from "@genesy/api-client/model"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { cn } from "@genesy/ui/lib/utils"
import { Keypad, type PadKey } from "@/components/keypad"
import { Receipt, Saved } from "@/components/receipt"
import {
  cartStore,
  estimate,
  toSaleLines,
  useCart,
  type CartLine,
} from "@/lib/cart-store"
import { queueStore } from "@/lib/sale-queue"
import { useTill } from "@/lib/till-store"
import { useTillCatalog, type Sellable } from "@/lib/use-catalog"
import { useTillMoney } from "@/lib/use-money"
import { useQuote } from "@/lib/use-quote"

const same = (
  a: { productId: string; variantId: string | null },
  b: { productId: string; variantId: string | null }
) => a.productId === b.productId && a.variantId === b.variantId

/** The API's figures for one basket line, from the quote. */
const quoted = (lines: QuoteLine[] | undefined, l: CartLine) =>
  lines?.find(
    (q) =>
      q.product_id === l.productId &&
      (q.product_variant_id ?? null) === l.variantId
  )

/**
 * Ring up a sale (TillSale A): scan or tap tiles, adjust the basket, then take cash or card in
 * the payment sheet, which also shows the receipt.
 */
export default function SalePage() {
  const t = useTranslations("sale")
  const tc = useTranslations("common")
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  const accepted = useTill((s) => s.session?.register.accepted_cash)
  const catalog = useTillCatalog()
  const { money } = useTillMoney()
  const lines = useCart((s) => s.lines)
  const [query, setQuery] = React.useState("")
  const [notFound, setNotFound] = React.useState<string | null>(null)
  const [sheet, setSheet] = React.useState<"pay" | "done" | "saved" | null>(
    null
  )
  const [method, setMethod] = React.useState<"cash" | "card">("cash")
  // Digits as typed on the pad: cash in minor units (no decimal key), the card slip as is.
  const [tendered, setTendered] = React.useState("")
  const [reference, setReference] = React.useState("")
  const [badTendered, setBadTendered] = React.useState(false)
  const [done, setDone] = React.useState<{
    sale: SaleResource
    lines: CartLine[]
    quoted?: QuoteLine[]
  } | null>(null)
  const [saved, setSaved] = React.useState<CartLine[] | null>(null)
  const sheetRef = React.useRef<HTMLElement>(null)
  const cash = method === "cash" && tendered ? Number(tendered) : null
  const { online, quote, error: quoteError } = useQuote(lines, method, cash)
  // Read up front so the cashier knows before charging; a 409 at charge still covers a late close.
  const shift = useTillShiftCurrent(workspace, { query: { enabled: online } })
  const noShift = shift.isSuccess && !shift.data.data
  const settling = online && shift.isPending

  const sale = useTillSalesCreate({
    mutation: {
      onSuccess: ({ data }) => {
        setDone({ sale: data, lines, quoted: quote?.lines })
        setSheet("done")
        cartStore.getState().clear()
        setTendered("")
        setReference("")
      },
    },
  })

  // The pad (and a hardware keyboard) belong to the sheet while it is open.
  React.useEffect(() => {
    if (sheet) sheetRef.current?.focus()
  }, [sheet])

  const add = (item: Sellable) => {
    sale.reset()
    cartStore.getState().add(item)
  }

  /** No answer from the API (offline, or the reply got lost): keep the sale for replay. */
  const keep = (body: SaleRequest) => {
    sale.reset()
    queueStore.getState().add({
      workspace,
      body: { ...body, sold_at: new Date().toISOString() },
      lines,
    })
    setSaved(lines)
    setSheet("saved")
    cartStore.getState().clear()
    setTendered("")
    setReference("")
  }

  const book = () => {
    if (sale.isPending) return
    setBadTendered(method === "cash" && cash == null)
    if (method === "cash" && cash == null) return
    const body: SaleRequest = {
      client_ref: cartStore.getState().startCharge(),
      lines: toSaleLines(lines),
      payment:
        method === "cash"
          ? { method: "cash", tendered: cash }
          : { method: "card", reference: reference.trim() || null },
    }
    if (!navigator.onLine) return keep(body)
    sale.mutate(
      { workspace, data: body },
      {
        onError: (e) => {
          if (!e.response) keep(body)
          else if (e.response.status === 409) void shift.refetch()
        },
      }
    )
  }

  const press = (k: PadKey) => {
    setBadTendered(false)
    if (method === "cash")
      setTendered((v) =>
        k === "back" ? v.slice(0, -1) : (v + k).replace(/^0+/, "").slice(0, 9)
      )
    else
      setReference((v) =>
        k === "back"
          ? v.slice(0, -1)
          : k === "clear"
            ? ""
            : (v + k).slice(0, 64)
      )
  }

  const closePay = () => {
    sale.reset()
    setBadTendered(false)
    setSheet(null)
  }
  const newSale = () => {
    sale.reset()
    setDone(null)
    setSaved(null)
    setSheet(null)
    setMethod("cash")
    setTendered("")
    setReference("")
    setBadTendered(false)
  }

  const shown = query.trim() ? catalog.search(query) : catalog.items
  const count = lines.reduce((n, l) => n + l.quantity, 0)
  const dueText = online ? money(quote?.due) : `≈ ${money(estimate(lines))}`
  // Which notes to offer: display only; the API works out the change.
  const cover = quote?.due ?? estimate(lines)
  const notes =
    cover == null
      ? []
      : [...(accepted ?? [])]
          .sort((a, b) => a - b)
          .filter((v) => v >= cover)
          .slice(0, 3)
  const status = sale.error?.response?.status
  const fieldErrors = sale.error?.response?.data
    ? Object.values(
        (sale.error.response.data as { errors?: Record<string, string[]> })
          .errors ?? {}
      ).flat()
    : []
  const close = sheet === "pay" ? closePay : newSale

  return (
    <main className="till-body sale">
      <h1 className="sr-only">{t("title")}</h1>
      <div className="pane">
        <form
          onSubmit={async (e) => {
            e.preventDefault()
            const code = query.trim()
            if (!code) return
            const item = await catalog.lookup(code)
            setNotFound(item ? null : code)
            if (item) {
              setQuery("")
              add(item)
            }
          }}
        >
          <label className="scan">
            <ScanBarcode aria-hidden className="size-[22px] shrink-0" />
            <input
              aria-label={t("scan")}
              placeholder={t("scan")}
              value={query}
              autoComplete="off"
              autoFocus
              onChange={(e) => setQuery(e.target.value)}
            />
            <span className="tbadge slate nodot ready no-phone">
              {t("ready")}
            </span>
          </label>
        </form>
        {notFound != null ? (
          <Notice variant="error" title={t("notFound", { code: notFound })} />
        ) : null}
        {/* Notices and tiles land together, below the scan field, so nothing above them moves. */}
        {settling ? null : (
          <>
            {noShift ? (
              <Notice
                variant="warning"
                title={t("noShift")}
                action={
                  <Link href="/drawer" className="badge-link underline">
                    {t("toDrawer")}
                  </Link>
                }
              />
            ) : null}
            {catalog.error ? (
              <Notice
                variant={catalog.count ? "warning" : "error"}
                title={catalog.count ? t("catalogStale") : t("catalogFailed")}
              />
            ) : null}
          </>
        )}
        {catalog.loading || settling ? (
          <div className="ptiles" aria-busy aria-label={t("catalogLoading")}>
            {Array.from({ length: 9 }, (_, i) => (
              <div key={i} className="ptile">
                <Skeleton className="h-5 w-3/4" />
                <Skeleton className="h-6 w-1/3" />
              </div>
            ))}
          </div>
        ) : catalog.empty ? (
          <EmptyState title={t("catalogEmpty")} />
        ) : query.trim() && !shown.length ? (
          <EmptyState title={t("noMatch", { query: query.trim() })} />
        ) : (
          <ul aria-label={t("products")} className="ptiles">
            {shown.map((item) => {
              const qty = lines.find((l) => same(l, item))?.quantity
              return (
                <li key={`${item.productId}-${item.variantId}`}>
                  <button
                    type="button"
                    className="ptile"
                    onClick={() => add(item)}
                  >
                    <span className="nm">{item.name}</span>
                    <span className="pr">{money(item.unitGross)}</span>
                    {qty ? (
                      <span
                        className="qty"
                        aria-label={t("inBasket", { count: qty })}
                      >
                        {qty}
                      </span>
                    ) : null}
                  </button>
                </li>
              )
            })}
          </ul>
        )}
      </div>

      <aside className="basket dock glass-strong" aria-label={t("basket")}>
        <div className="flex min-h-0 flex-col gap-2">
          <div className="flex items-center justify-between gap-3">
            <p className="card-title">{t("basketCount", { count })}</p>
            <Button
              variant="ghost"
              className="btn-44"
              disabled={!lines.length}
              onClick={() => cartStore.getState().clear()}
            >
              {t("clear")}
            </Button>
          </div>
          {lines.length === 0 ? (
            <EmptyState title={t("empty")} className="py-6" />
          ) : (
            <ul className="lines">
              {lines.map((l, i) => (
                <li key={`${l.productId}-${l.variantId}`} className="line">
                  <span className="ln">{l.name}</span>
                  <span className="lp">
                    {online
                      ? money(quoted(quote?.lines, l)?.line_total)
                      : `≈ ${money(estimate([l]))}`}
                  </span>
                  <span className="qstep">
                    <button
                      type="button"
                      aria-label={t("less", { name: l.name })}
                      onClick={() =>
                        l.quantity > 1
                          ? cartStore.getState().setQuantity(i, l.quantity - 1)
                          : cartStore.getState().remove(i)
                      }
                    >
                      <Minus aria-hidden className="size-[18px]" />
                    </button>
                    <span aria-label={t("quantityOf", { name: l.name })}>
                      {l.quantity}
                    </span>
                    <button
                      type="button"
                      aria-label={t("more", { name: l.name })}
                      onClick={() =>
                        cartStore.getState().setQuantity(i, l.quantity + 1)
                      }
                    >
                      <Plus aria-hidden className="size-[18px]" />
                    </button>
                  </span>
                  <span className="text-right text-xs text-muted-foreground">
                    {t("each", { price: money(l.unitGross) })}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="mt-auto flex flex-col gap-2.5">
          <ApiError error={quoteError} fallback={tc("error")} />
          {quote?.mismatched_lines ? (
            <Notice variant="warning" title={t("mismatch")} />
          ) : null}
          {lines.length === 0 ? null : online ? (
            <dl aria-label={t("quote")} className="totals">
              <div className="text-[13px]">
                <dt className="text-muted-foreground">{t("total")}</dt>
                <dd className="tabular-nums">{money(quote?.total)}</dd>
              </div>
              {quote?.rounding ? (
                <div className="text-[13px]">
                  <dt className="text-muted-foreground">{t("rounding")}</dt>
                  <dd className="tabular-nums">{money(quote.rounding)}</dd>
                </div>
              ) : null}
              <div>
                <dt className="font-semibold">{t("due")}</dt>
                <dd className="grand">{money(quote?.due)}</dd>
              </div>
            </dl>
          ) : (
            <dl aria-label={t("quote")} className="totals">
              <div>
                <dt className="font-semibold">{t("estimate")}</dt>
                <dd className="grand">{dueText}</dd>
              </div>
              <p className="text-xs text-muted-foreground">
                {t("estimateHint")}
              </p>
            </dl>
          )}
          <button
            type="button"
            className="charge"
            disabled={lines.length === 0}
            onClick={() => {
              sale.reset()
              setSheet("pay")
            }}
          >
            <span>{t("charge")}</span>
            <span className="tabular-nums">
              {lines.length ? dueText : money(0)}
            </span>
          </button>
        </div>
      </aside>

      {sheet ? (
        <>
          <div className="scrim" onClick={close} />
          <section
            ref={sheetRef}
            tabIndex={-1}
            role="dialog"
            aria-modal
            aria-labelledby="sheet-title"
            className={cn("sheet glass-strong", sheet === "pay" && "pay")}
            onKeyDown={(e) => {
              if (e.key === "Escape") close()
            }}
          >
            <div className="sheet-head flex items-center justify-between gap-3">
              <h2 id="sheet-title" className="card-title text-xl">
                {sheet === "pay"
                  ? t("pay")
                  : sheet === "done"
                    ? t("done")
                    : t("savedTitle")}
              </h2>
              <Button
                variant="outline"
                size="icon"
                className="btn-44 size-12 rounded-[14px]"
                aria-label={t("close")}
                onClick={close}
              >
                <X aria-hidden />
              </Button>
            </div>

            {sheet === "pay" ? (
              <>
                <div className="sheet-main sheet-part">
                  <div className="seg" role="group" aria-label={t("method")}>
                    {(["cash", "card"] as const).map((m) => (
                      <button
                        key={m}
                        type="button"
                        aria-pressed={method === m}
                        onClick={() => {
                          sale.reset()
                          setBadTendered(false)
                          setMethod(m)
                        }}
                      >
                        {m === "cash" ? (
                          <Banknote aria-hidden className="size-[18px]" />
                        ) : (
                          <CreditCard aria-hidden className="size-[18px]" />
                        )}
                        {t(m)}
                      </button>
                    ))}
                  </div>
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-muted-foreground">{t("due")}</span>
                    <span className="grand">{dueText}</span>
                  </div>
                  {method === "cash" ? (
                    <>
                      <div className="flex flex-col gap-1.5">
                        <span
                          id="tender-label"
                          className="text-[13px] font-medium"
                        >
                          {t("received")}
                        </span>
                        <output
                          aria-labelledby="tender-label"
                          className="tender"
                        >
                          {money(cash ?? 0)}
                        </output>
                      </div>
                      <div className="notes">
                        <Button
                          variant="outline"
                          className="h-[52px] rounded-[14px] font-semibold"
                          disabled={quote == null}
                          onClick={() => {
                            setBadTendered(false)
                            setTendered(String(quote!.due))
                          }}
                        >
                          {t("exact")}
                        </Button>
                        {notes.map((v) => (
                          <Button
                            key={v}
                            variant="outline"
                            className="h-[52px] rounded-[14px] font-semibold tabular-nums"
                            onClick={() => {
                              setBadTendered(false)
                              setTendered(String(v))
                            }}
                          >
                            {money(v)}
                          </Button>
                        ))}
                      </div>
                    </>
                  ) : (
                    <div className="flex flex-col gap-1.5">
                      <span
                        id="tender-label"
                        className="text-[13px] font-medium"
                      >
                        {t("reference")}
                      </span>
                      <output
                        aria-labelledby="tender-label"
                        className="tender mono"
                      >
                        {reference}
                      </output>
                      <span className="text-xs text-muted-foreground">
                        {t("referenceHint")}
                      </span>
                    </div>
                  )}
                </div>
                <Keypad
                  fn={method === "cash" ? "00" : "clear"}
                  keyboard
                  onKey={press}
                  onEnter={book}
                />
                <div className="sheet-foot">
                  {method === "cash" ? (
                    <div className="teal-tile">
                      <span id="change-label" className="font-semibold">
                        {t("change")}
                      </span>
                      <output aria-labelledby="change-label" className="grand">
                        {money(quote?.change)}
                      </output>
                    </div>
                  ) : null}
                  {badTendered ? (
                    <Notice variant="error" title={t("badTendered")} />
                  ) : null}
                  {sale.error ? (
                    <Notice
                      variant="error"
                      title={
                        (
                          sale.error.response?.data as
                            { message?: string } | undefined
                        )?.message ?? tc("error")
                      }
                      action={
                        status === 409 ? (
                          <Link href="/drawer" className="badge-link underline">
                            {t("toDrawer")}
                          </Link>
                        ) : null
                      }
                    >
                      {fieldErrors.length
                        ? fieldErrors.map((m) => <p key={m}>{m}</p>)
                        : null}
                    </Notice>
                  ) : null}
                  <button
                    type="button"
                    className="charge"
                    disabled={sale.isPending}
                    onClick={book}
                  >
                    <span>{method === "cash" ? t("book") : t("bookCard")}</span>
                    <span className="tabular-nums">{dueText}</span>
                  </button>
                </div>
              </>
            ) : sheet === "done" && done ? (
              <Receipt
                sale={done.sale}
                lines={done.lines}
                quoted={done.quoted}
                onDone={newSale}
              />
            ) : saved ? (
              <Saved lines={saved} onDone={newSale} />
            ) : null}
          </section>
        </>
      ) : null}
    </main>
  )
}
