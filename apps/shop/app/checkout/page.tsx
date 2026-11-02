import type { Metadata } from "next"
import { getTranslations } from "next-intl/server"

import { cn } from "@genesy/ui/lib/utils"

import { CheckoutForm } from "@/components/checkout-form"
import { h2 } from "@/lib/look"

export async function generateMetadata(): Promise<Metadata> {
  return {
    title: (await getTranslations("checkout"))("title"),
    robots: { index: false },
  }
}

const STEPS = ["cart", "details", "pay"] as const
/** This page is step 2: the cart is done, paying comes after the order page. */
const CURRENT = 1

/** Mock `ShopCheckout`: title and steps, then the form tiles. */
export default async function CheckoutPage() {
  const t = await getTranslations("checkout")
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className={h2}>{t("title")}</h1>
        <ol
          aria-label={t("steps")}
          className="flex items-center gap-2 text-[13px]"
        >
          {STEPS.map((step, i) => (
            <li
              key={step}
              aria-current={i === CURRENT ? "step" : undefined}
              className={cn(
                "flex items-center gap-2",
                i === CURRENT
                  ? "font-semibold text-foreground"
                  : "text-muted-foreground"
              )}
            >
              {i ? <span aria-hidden className="h-px w-7 bg-border" /> : null}
              <b
                className={cn(
                  "inline-flex size-6 items-center justify-center rounded-full border-[1.5px] text-xs",
                  i < CURRENT &&
                    "border-transparent bg-teal/13 text-(--tone-success) dark:bg-teal/22",
                  i === CURRENT &&
                    "border-primary bg-primary text-primary-foreground",
                  i > CURRENT && "border-border"
                )}
              >
                {i < CURRENT ? "✓" : i + 1}
              </b>
              <span className="max-md:sr-only">{t(`step.${step}`)}</span>
            </li>
          ))}
        </ol>
      </div>
      <CheckoutForm />
    </>
  )
}
