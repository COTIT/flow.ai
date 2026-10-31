"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"

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
      <div className="flex flex-col items-start gap-4">
        <p className="text-muted-foreground">{t("empty")}</p>
        <Link href="/" className={buttonVariants()}>
          {t("shop")}
        </Link>
      </div>
    )

  // API paths like `lines.1.quantity` name a cart line; show its product name with the message.
  const apiMessages = Object.entries(state.errors ?? {}).flatMap(
    ([path, messages]) => {
      const line = /^lines\.(\d+)\./.exec(path)?.[1]
      const name = line ? lines[Number(line)]?.name : null
      return messages.map((m) => (name ? `${name}: ${m}` : m))
    }
  )
  const problem = state.problem ? t(`problem.${state.problem}`) : null

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
        {problem || apiMessages.length ? (
          <div
            role="alert"
            className="rounded-lg border border-destructive p-4 text-destructive"
          >
            {problem ? <p>{problem}</p> : null}
            {apiMessages.length ? (
              <ul className="list-inside list-disc">
                {apiMessages.map((m) => (
                  <li key={m}>{m}</li>
                ))}
              </ul>
            ) : null}
          </div>
        ) : null}
        {FIELDS.map(([name, type, autoComplete]) => {
          const missing = state.missing?.includes(name)
          return (
            <div key={name} className="flex flex-col gap-1">
              <Label htmlFor={name}>{t(`field.${name}`)}</Label>
              <Input
                id={name}
                name={name}
                type={type}
                autoComplete={autoComplete}
                maxLength={255}
                aria-invalid={missing || undefined}
                aria-describedby={missing ? `${name}-error` : undefined}
              />
              {missing ? (
                <p id={`${name}-error`} className="text-sm text-destructive">
                  {t("required")}
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
