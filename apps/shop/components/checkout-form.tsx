"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { Button, buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"
import { Notice } from "@genesy/ui/components/notice"

import { placeOrder, type CheckoutState } from "@/app/actions"
import { useCart } from "@/lib/use-cart"

const FIELDS = [
  ["email", "email", "email"],
  ["name", "text", "name"],
  ["phone", "tel", "tel"],
  ["line1", "text", "address-line1"],
  ["line2", "text", "address-line2"],
  ["postal_code", "text", "postal-code"],
  ["city", "text", "address-level2"],
  ["country", "text", "country"],
] as const

export function CheckoutForm() {
  const t = useTranslations("checkout")
  const tc = useTranslations("cart")
  const { lines, ready } = useCart()
  // One key per visit: a double submit or a retry places one order, not two.
  const [key] = React.useState(() => crypto.randomUUID())
  const [state, action, pending] = React.useActionState<
    CheckoutState,
    FormData
  >(placeOrder, {})

  if (!ready) return null
  if (!lines.length)
    return (
      <EmptyState
        title={tc("empty")}
        description={tc("emptyWhy")}
        action={
          <Link href="/" className={buttonVariants()}>
            {tc("browse")}
          </Link>
        }
      />
    )

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

  return (
    <form
      action={action}
      className="grid gap-8 md:grid-cols-[1fr_20rem]"
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
      <div className="flex flex-col gap-4">
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
        {FIELDS.map(([name, type, autoComplete]) => {
          const missing = state.missing?.includes(name)
          const messages = missing ? [t("required")] : fieldMessages[name]
          return (
            <div key={name} className="flex flex-col gap-1">
              <Label htmlFor={name}>{t(`field.${name}`)}</Label>
              <Input
                id={name}
                name={name}
                type={type}
                autoComplete={autoComplete}
                maxLength={255}
                aria-invalid={messages ? true : undefined}
                aria-describedby={messages ? `${name}-error` : undefined}
              />
              {messages ? (
                <p id={`${name}-error`} className="text-sm text-destructive">
                  {messages.join(" ")}
                </p>
              ) : null}
            </div>
          )
        })}
      </div>
      <aside className="flex h-fit flex-col gap-4 rounded-lg bg-muted p-4">
        <h2 className="heading-3">{t("summary")}</h2>
        <ul className="flex flex-col gap-1 text-sm">
          {lines.map((l) => (
            <li
              key={`${l.productId}:${l.variantId}`}
              className="flex justify-between gap-2"
            >
              <span>
                {l.name}
                {l.variantLabel ? ` (${l.variantLabel})` : ""}
              </span>
              <span className="tabular-nums">× {l.quantity}</span>
            </li>
          ))}
        </ul>
        <p className="text-sm text-muted-foreground">{t("totalNext")}</p>
        <Button type="submit" size="lg" disabled={pending}>
          {t("place")}
        </Button>
      </aside>
    </form>
  )
}
