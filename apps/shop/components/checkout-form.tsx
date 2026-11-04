"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { formatMoney } from "@genesy/ui/lib/money"
import { cn } from "@genesy/ui/lib/utils"

import { placeOrder, type CheckoutState } from "@/app/actions"
import { card, cardTitle, honey } from "@/lib/look"
import { useCart } from "@/lib/use-cart"

import { EmptyCart } from "./cart-view"

type Field = readonly [name: string, type: string, autoComplete: string]
const CONTACT: Field[] = [
  ["email", "email", "email"],
  ["phone", "tel", "tel"],
  ["name", "text", "name"],
]
const ADDRESS: Field[] = [
  ["line1", "text", "address-line1"],
  ["line2", "text", "address-line2"],
]
const PLACE: Field[] = [
  ["postal_code", "text", "postal-code"],
  ["city", "text", "address-level2"],
  ["country", "text", "country"],
]
const FIELDS = [...CONTACT, ...ADDRESS, ...PLACE]
/** Under its field, as drawn: only the email has a hint. */
const HINTED = new Set(["email"])

const thumb = cn(honey, "size-12 shrink-0 rounded-[12px]")

/** Mock `ShopCheckout`: Contact and Shipping address cards beside "Your order". */
export function CheckoutForm() {
  const t = useTranslations("checkout")
  const locale = useLocale()
  const { lines, ready } = useCart()
  // One key per visit: a double submit or a retry places one order, not two.
  const [key] = React.useState(() => crypto.randomUUID())
  const [state, action, pending] = React.useActionState<
    CheckoutState,
    FormData
  >(placeOrder, {})

  if (ready && !lines.length) return <EmptyCart />

  // The API's 422 paths: customer fields go under their input, the rest (e.g.
  // `lines.1.quantity`, named by the cart line's product) into the notice on top.
  const fieldMessages: Record<string, string[]> = {}
  const orderMessages: string[] = []
  for (const [path, messages] of Object.entries(state.errors ?? {})) {
    const field = /^customer\.(?:shipping_address\.)?(\w+)$/.exec(path)?.[1]
    if (field && FIELDS.some(([name]) => name === field))
      fieldMessages[field] = messages
    else {
      const line = /^lines\.(\d+)\./.exec(path)?.[1]
      const name = line ? lines[Number(line)]?.name : null
      orderMessages.push(...messages.map((m) => (name ? `${name}: ${m}` : m)))
    }
  }
  const fieldsWrong =
    Object.keys(fieldMessages).length > 0 || (state.missing?.length ?? 0) > 0

  const field = ([name, type, autoComplete]: Field, className?: string) => {
    const missing = state.missing?.includes(name)
    // The API's text is English only: a translated line first, its words after as the detail.
    const messages = missing
      ? [t("required")]
      : fieldMessages[name] && [t("invalid"), ...fieldMessages[name]]
    // The error first: it is what the buyer must act on.
    const described = [
      messages && `${name}-error`,
      HINTED.has(name) && `${name}-hint`,
    ]
      .filter(Boolean)
      .join(" ")
    return (
      <div
        key={name}
        className={cn("flex min-w-0 flex-col gap-1.5", className)}
      >
        <Label htmlFor={name} className="text-[13px]">
          {t(`field.${name}`)}
        </Label>
        <Input
          id={name}
          name={name}
          type={type}
          autoComplete={autoComplete}
          maxLength={255}
          aria-invalid={messages ? true : undefined}
          aria-describedby={described || undefined}
        />
        {HINTED.has(name) ? (
          <p id={`${name}-hint`} className="text-xs text-muted-foreground">
            {t(`hint.${name}`)}
          </p>
        ) : null}
        {messages ? (
          <p
            id={`${name}-error`}
            className="text-xs font-medium text-(--tone-error)"
          >
            {messages.join(" ")}
          </p>
        ) : null}
      </div>
    )
  }

  return (
    <form
      action={action}
      className="grid grid-cols-12 gap-3 md:gap-4"
      noValidate
    >
      <input type="hidden" name="key" value={key} />
      <input
        type="hidden"
        name="lines"
        value={JSON.stringify(
          lines.map(({ productId, variantId, quantity }) => ({
            productId,
            variantId,
            quantity,
          }))
        )}
      />
      <div className="col-span-12 flex min-w-0 flex-col gap-4 md:col-span-8">
        {state.problem ? (
          <Notice
            // Too busy is temporary and not the buyer's fault: a warning, not an error.
            variant={state.problem === "busy" ? "warning" : "error"}
            title={t(`problem.${state.problem}`)}
          />
        ) : orderMessages.length ? (
          <Notice variant="error" title={t("orderTitle")}>
            <ul className="list-inside list-disc">
              {orderMessages.map((m) => (
                <li key={m}>{m}</li>
              ))}
            </ul>
          </Notice>
        ) : fieldsWrong ? (
          <Notice variant="error" title={t("fieldsTitle")} />
        ) : null}
        <section aria-labelledby="checkout-contact" className={card}>
          <h2 id="checkout-contact" className={cardTitle}>
            {t("contact")}
          </h2>
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            {CONTACT.map((f) =>
              field(f, f[0] === "name" ? "md:col-span-2" : undefined)
            )}
          </div>
        </section>
        <section aria-labelledby="checkout-address" className={card}>
          <h2 id="checkout-address" className={cardTitle}>
            {t("address")}
          </h2>
          {ADDRESS.map((f) => field(f))}
          <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
            {PLACE.map((f) => field(f))}
          </div>
        </section>
      </div>

      <section
        aria-labelledby="checkout-order"
        className={cn(card, "col-span-12 md:col-span-4")}
      >
        <h2 id="checkout-order" className={cardTitle}>
          {t("summary")}
        </h2>
        {ready ? (
          <ul className="flex flex-col gap-3">
            {lines.map((l) => (
              <li
                key={`${l.productId}:${l.variantId}`}
                className="flex items-center gap-3"
              >
                <span aria-hidden className={thumb} />
                <div className="min-w-0 flex-auto">
                  <p className="text-[13px] font-semibold">
                    {l.name}
                    {l.variantLabel ? ` · ${l.variantLabel}` : ""}
                  </p>
                  <p className="text-xs text-muted-foreground tabular-nums">
                    {l.quantity} ×{" "}
                    {formatMoney(l.priceGross, l.decimals, l.currency, locale)}
                  </p>
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <div aria-hidden className="flex items-center gap-3">
            <Skeleton className={thumb} />
            <div className="flex flex-auto flex-col gap-2">
              <Skeleton className="h-3.5 w-3/4" />
              <Skeleton className="h-3 w-1/3" />
            </div>
          </div>
        )}
        <div className="h-px bg-(--hair)" />
        <p className="text-xs text-muted-foreground">{t("totalNext")}</p>
        <Button
          type="submit"
          disabled={!ready || pending}
          size="lg"
          className="w-full"
        >
          {t("place")}
        </Button>
      </section>
    </form>
  )
}
