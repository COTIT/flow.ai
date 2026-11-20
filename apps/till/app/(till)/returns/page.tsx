"use client"

import * as React from "react"
import { Banknote, CreditCard, Minus, Plus, Search } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  useTillRefundsCreate,
  useTillRefundsCreateReview,
  useTillSalesLookup,
  useTillSalesReturn,
  useTillSalesReturnReview,
} from "@verzo/api-client/endpoints/till"
import type {
  RefundReview,
  ReturnReview,
  TillSaleLookup,
  TillSaleLookupCredit,
} from "@verzo/api-client/model"
import { useIdempotencyKey } from "@verzo/api-client/idempotency"
import { Button } from "@verzo/ui/components/button"
import { Checkbox } from "@verzo/ui/components/checkbox"
import { Input } from "@verzo/ui/components/input"
import { Label } from "@verzo/ui/components/label"
import { Notice } from "@verzo/ui/components/notice"
import { Skeleton } from "@verzo/ui/components/skeleton"
import { toMinor } from "@verzo/ui/lib/money"
import { ApprovalGate } from "@/components/approval-gate"
import { Refused } from "@/components/refused"
import { Keypad, type PadKey } from "@/components/keypad"
import { privileged, useApproval } from "@/lib/approval"
import { useTill } from "@/lib/till-store"
import { useTillMoney } from "@/lib/use-money"

/**
 * Returns and refunds (privileged): a member approves with their own login, the cashier finds
 * the sale by its receipt number, takes lines back (the API's review says the credit), then
 * pays the credit back in cash from this drawer or records a card refund the terminal made.
 */
export default function ReturnsPage() {
  return (
    <main className="till-center items-start">
      <div className="flex w-[min(980px,100%)] flex-col gap-3">
        <ApprovalGate right="cashregister.refund">
          <Returns />
        </ApprovalGate>
      </div>
    </main>
  )
}

function Returns() {
  const t = useTranslations("returns")
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  const approval = useApproval()
  const [number, setNumber] = React.useState("")
  const [searched, setSearched] = React.useState("")
  const lookup = useTillSalesLookup(
    workspace,
    { number: searched },
    {
      query: { enabled: searched !== "", retry: false, staleTime: 0 },
      request: privileged(approval),
    }
  )
  const sale = lookup.data?.data
  const notFound =
    (lookup.error as { response?: { status?: number } } | null)?.response
      ?.status === 404

  return (
    <>
      <form
        className="till-card tight"
        role="search"
        onSubmit={(e) => {
          e.preventDefault()
          if (number.trim() === searched) void lookup.refetch()
          setSearched(number.trim())
        }}
      >
        <Label htmlFor="receipt-number" className="till-h3">
          {t("findTitle")}
        </Label>
        <div className="flex gap-2">
          <Input
            id="receipt-number"
            className="h-12 min-w-0 flex-1 text-base"
            autoComplete="off"
            placeholder={t("numberHint")}
            value={number}
            onChange={(e) => setNumber(e.target.value)}
          />
          <Button
            type="submit"
            className="btn-44 h-12"
            disabled={!number.trim()}
          >
            <Search aria-hidden /> {t("find")}
          </Button>
        </div>
      </form>
      {lookup.isFetching && !sale ? (
        <div className="till-card gap-3" aria-busy aria-label={t("loading")}>
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </div>
      ) : notFound ? (
        <Notice variant="info" title={t("notFound", { number: searched })} />
      ) : lookup.error ? (
        <Refused error={lookup.error} fallback={t("lookupFailed")} />
      ) : sale ? (
        <Sale
          key={sale.order_id}
          sale={sale}
          refresh={() => void lookup.refetch()}
        />
      ) : null}
    </>
  )
}

function Sale({
  sale,
  refresh,
}: {
  sale: TillSaleLookup
  refresh: () => void
}) {
  const t = useTranslations("returns")
  const { money } = useTillMoney()
  const open = sale.credits.filter((c) => c.refundable > 0)
  const [picked, setPicked] = React.useState<string | null>(null)
  const [refunded, setRefunded] = React.useState<number | null>(null)
  // One credit left to refund: it is the one (also right after a return booked it).
  const refunding =
    picked ?? (open.length === 1 ? open[0]!.credit_open_item_id : null)
  const sold = new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(sale.sold_at))
  const credit = sale.credits.find(
    (c) => c.credit_open_item_id === refunding && c.refundable > 0
  )

  return (
    <div className="split split-wide p-0">
      <section className="till-card" aria-labelledby="sale-title">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 id="sale-title" className="till-h2">
            {t("saleTitle", { number: sale.number })}
          </h2>
          <span className="tbadge slate">{sale.register_name}</span>
        </div>
        <p className="text-sm text-muted-foreground">
          {t("soldAt", { when: sold, total: money(sale.due) })}
        </p>
        <ReturnForm sale={sale} onPosted={refresh} />
      </section>
      <section
        className="flex min-w-0 flex-col gap-3"
        aria-label={t("credits")}
      >
        <div className="till-card tight">
          <h2 className="till-h3">{t("credits")}</h2>
          {sale.credits.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("noCredits")}</p>
          ) : (
            <ul className="flex flex-col">
              {sale.credits.map((c, i) => (
                <li key={c.credit_open_item_id} className="line">
                  <span className="ln">{t("creditN", { n: i + 1 })}</span>
                  <span className="lp">{money(c.refundable)}</span>
                  <span className="text-xs text-muted-foreground">
                    {c.refundable > 0 ? t("leftToRefund") : t("refunded")}
                  </span>
                  {c.refundable > 0 ? (
                    <Button
                      variant={
                        refunding === c.credit_open_item_id
                          ? "default"
                          : "outline"
                      }
                      className="btn-44 justify-self-end"
                      aria-pressed={refunding === c.credit_open_item_id}
                      onClick={() => setPicked(c.credit_open_item_id)}
                    >
                      {t("refundThis")}
                    </Button>
                  ) : (
                    <span />
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
        {refunded != null ? (
          <Notice
            variant="success"
            title={t("refundedAmount", { amount: money(refunded) })}
          />
        ) : null}
        {credit ? (
          <RefundForm
            key={`${credit.credit_open_item_id}-${credit.refundable}`}
            sale={sale}
            credit={credit}
            onPosted={(amount) => {
              setRefunded(amount)
              refresh()
            }}
          />
        ) : null}
      </section>
    </div>
  )
}

function ReturnForm({
  sale,
  onPosted,
}: {
  sale: TillSaleLookup
  onPosted: () => void
}) {
  const t = useTranslations("returns")
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  const approval = useApproval()
  const idem = useIdempotencyKey()
  const { money } = useTillMoney()
  const [qty, setQty] = React.useState<Record<string, number>>({})
  const [restock, setRestock] = React.useState(true)
  const [reason, setReason] = React.useState("")
  const [review, setReview] = React.useState<ReturnReview | null>(null)
  const [posted, setPosted] = React.useState(false)
  const auth = privileged(approval)
  const preview = useTillSalesReturnReview({ request: auth })
  const post = useTillSalesReturn({
    request: { headers: { ...auth.headers, ...idem.request.headers } },
    mutation: {
      onSettled: (_, e) => idem.settle(e),
      onSuccess: () => {
        setReview(null)
        setQty({})
        setReason("")
        setPosted(true)
        onPosted()
      },
    },
  })
  const lines = sale.lines
    .filter((l) => (qty[l.id] ?? 0) > 0)
    .map((l) => ({ original_line_id: l.id, quantity: String(qty[l.id]) }))
  const body = { lines, restock, reason: reason.trim() }
  const change = (id: string, n: number) => {
    setReview(null)
    setPosted(false)
    preview.reset()
    post.reset()
    setQty({ ...qty, [id]: n })
  }
  // shortcut: whole units only (till lines are counted pieces); a weighed line returns in the Office.
  const returnable = (q: string) => Math.floor(Number(q))

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault()
        if (!review)
          preview.mutate(
            { workspace, order: sale.order_id, data: body },
            { onSuccess: ({ data }) => setReview(data) }
          )
        else
          post.mutate({
            workspace,
            order: sale.order_id,
            data: {
              client_ref: idem.request.headers["Idempotency-Key"],
              payload: {
                ...body,
                review_fingerprint: review.review_fingerprint,
              },
            },
          })
      }}
    >
      <ul className="lines" aria-label={t("lines")}>
        {sale.lines.map((l) => {
          const max = returnable(l.returnable)
          const n = qty[l.id] ?? 0
          return (
            <li key={l.id} className="line">
              <span className="ln">{l.title}</span>
              <span className="qstep justify-self-end">
                <button
                  type="button"
                  aria-label={t("less", { title: l.title })}
                  disabled={n === 0}
                  onClick={() => change(l.id, n - 1)}
                >
                  <Minus aria-hidden className="size-[18px]" />
                </button>
                <span aria-label={t("returnQty", { title: l.title })}>{n}</span>
                <button
                  type="button"
                  aria-label={t("more", { title: l.title })}
                  disabled={n >= max}
                  onClick={() => change(l.id, n + 1)}
                >
                  <Plus aria-hidden className="size-[18px]" />
                </button>
              </span>
              <span className="text-xs text-muted-foreground">
                {t("soldLeft", { sold: l.quantity, left: l.returnable })}
              </span>
            </li>
          )
        })}
      </ul>
      <div className="flex items-center gap-2">
        <Checkbox
          id="restock"
          checked={restock}
          onCheckedChange={(v) => {
            setReview(null)
            setRestock(v === true)
          }}
        />
        <Label htmlFor="restock">{t("restock")}</Label>
      </div>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="return-reason">{t("reason")}</Label>
        <Input
          id="return-reason"
          className="h-12"
          value={reason}
          maxLength={1000}
          onChange={(e) => {
            setReview(null)
            setReason(e.target.value)
          }}
        />
      </div>
      {posted ? <Notice variant="success" title={t("returned")} /> : null}
      {review ? (
        <div
          className="till-card strong tight"
          role="region"
          aria-label={t("reviewTitle")}
        >
          <h3 className="till-h3">{t("reviewTitle")}</h3>
          {review.returns.map((r) => (
            <div key={r.id} className="flex items-center justify-between gap-3">
              <span className="text-muted-foreground">{t("creditTotal")}</span>
              <output className="grand">
                {money(toMinor(r.credit ?? "", sale.decimals))}
              </output>
            </div>
          ))}
        </div>
      ) : null}
      {preview.error ? (
        <Refused error={preview.error} fallback={t("returnRefused")} />
      ) : null}
      {post.error ? (
        <Refused error={post.error} fallback={t("returnRefused")} />
      ) : null}
      <button
        type="submit"
        className="charge"
        disabled={
          lines.length === 0 ||
          !reason.trim() ||
          preview.isPending ||
          post.isPending
        }
      >
        <span>{review ? t("postReturn") : t("reviewReturn")}</span>
      </button>
    </form>
  )
}

function RefundForm({
  sale,
  credit,
  onPosted,
}: {
  sale: TillSaleLookup
  credit: TillSaleLookupCredit
  onPosted: (amount: number) => void
}) {
  const t = useTranslations("returns")
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  const register = useTill((s) => s.session?.register.id)
  const approval = useApproval()
  const idem = useIdempotencyKey()
  const { money } = useTillMoney()
  // Cash goes out of the drawer that took it; elsewhere only a card refund can be recorded.
  const cashHere = register === sale.register_id
  const [method, setMethod] = React.useState<"cash" | "card_recorded">(
    cashHere ? "cash" : "card_recorded"
  )
  // Digits as typed on the pad: minor units, as the sale's cash.
  const [amount, setAmount] = React.useState("")
  const [reference, setReference] = React.useState("")
  const [confirmed, setConfirmed] = React.useState(false)
  const [reason, setReason] = React.useState("")
  const [review, setReview] = React.useState<RefundReview | null>(null)
  const auth = privileged(approval)
  const preview = useTillRefundsCreateReview({ request: auth })
  const post = useTillRefundsCreate({
    request: { headers: { ...auth.headers, ...idem.request.headers } },
    mutation: {
      onSettled: (_, e) => idem.settle(e),
      onSuccess: ({ data }) => onPosted(data.amount),
    },
  })
  const card = method === "card_recorded"
  const body = {
    credit_open_item_id: credit.credit_open_item_id,
    amount: Number(amount),
    method,
    external_reference: card ? reference.trim() || null : null,
    actual_refund_confirmed: card ? confirmed : null,
  }
  const edit = () => {
    setReview(null)
    preview.reset()
    post.reset()
  }
  const press = (k: PadKey) => {
    edit()
    setAmount((a) =>
      k === "back"
        ? a.slice(0, -1)
        : k === "clear"
          ? ""
          : (a + k).replace(/^0+/, "").slice(0, 12)
    )
  }

  return (
    <form
      className="till-card strong tight"
      aria-label={t("refundTitle")}
      onSubmit={(e) => {
        e.preventDefault()
        if (!review)
          preview.mutate(
            { workspace, order: sale.order_id, data: body },
            { onSuccess: ({ data }) => setReview(data) }
          )
        else
          post.mutate({
            workspace,
            order: sale.order_id,
            data: {
              client_ref: idem.request.headers["Idempotency-Key"],
              payload: {
                ...body,
                reason: reason.trim(),
                review_fingerprint: review.review_fingerprint,
              },
            },
          })
      }}
    >
      <h2 className="till-h3">{t("refundTitle")}</h2>
      <div className="seg" role="group" aria-label={t("method")}>
        {(["cash", "card_recorded"] as const).map((m) => (
          <button
            key={m}
            type="button"
            aria-pressed={method === m}
            disabled={m === "cash" && !cashHere}
            onClick={() => {
              edit()
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
      {cashHere ? null : (
        <Notice
          variant="info"
          title={t("cashElsewhere", { register: sale.register_name })}
        />
      )}
      <div className="flex items-center justify-between gap-3">
        <span id="refundable-label" className="text-muted-foreground">
          {t("refundable")}
        </span>
        <output
          aria-labelledby="refundable-label"
          className="font-semibold tabular-nums"
        >
          {money(credit.refundable)}
        </output>
      </div>
      <div className="flex items-center justify-between gap-3">
        <span id="amount-label" className="font-semibold">
          {t("amount")}
        </span>
        <output aria-labelledby="amount-label" className="grand">
          {money(Number(amount || 0))}
        </output>
      </div>
      <Button
        type="button"
        variant="outline"
        className="h-[52px] rounded-[14px] font-semibold"
        onClick={() => {
          edit()
          setAmount(String(credit.refundable))
        }}
      >
        {t("all")}
      </Button>
      {method === "cash" && sale.cash_step > 1 ? (
        <p className="text-xs text-muted-foreground">
          {t("stepHint", { step: money(sale.cash_step) })}
        </p>
      ) : null}
      <Keypad fn="clear" onKey={press} />
      {card ? (
        <>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="refund-reference">{t("reference")}</Label>
            <Input
              id="refund-reference"
              className="h-12"
              autoComplete="off"
              maxLength={255}
              value={reference}
              onChange={(e) => {
                edit()
                setReference(e.target.value)
              }}
            />
          </div>
          <div className="flex items-center gap-2">
            <Checkbox
              id="refund-confirmed"
              checked={confirmed}
              onCheckedChange={(v) => {
                edit()
                setConfirmed(v === true)
              }}
            />
            <Label htmlFor="refund-confirmed">{t("confirmed")}</Label>
          </div>
        </>
      ) : null}
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="refund-reason">{t("reason")}</Label>
        <Input
          id="refund-reason"
          className="h-12"
          maxLength={1000}
          value={reason}
          onChange={(e) => {
            edit()
            setReason(e.target.value)
          }}
        />
      </div>
      {review ? (
        <div role="region" aria-label={t("reviewTitle")} className="totals">
          <div>
            <span className="text-muted-foreground">{t("payOut")}</span>
            <output className="font-semibold tabular-nums">
              {money(review.amount)}
            </output>
          </div>
          {review.drawer_expected != null ? (
            <div>
              <span className="text-muted-foreground">{t("drawerHolds")}</span>
              <output className="font-semibold tabular-nums">
                {money(review.drawer_expected)}
              </output>
            </div>
          ) : null}
        </div>
      ) : null}
      {preview.error ? (
        <Refused error={preview.error} fallback={t("refundRefused")} />
      ) : null}
      {post.error ? (
        <Refused error={post.error} fallback={t("refundRefused")} />
      ) : null}
      <button
        type="submit"
        className="charge"
        disabled={
          !amount || !reason.trim() || preview.isPending || post.isPending
        }
      >
        <span>{review ? t("payRefund") : t("reviewRefund")}</span>
        <span className="tabular-nums">{money(Number(amount || 0))}</span>
      </button>
    </form>
  )
}
