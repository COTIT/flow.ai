import type { Metadata } from "next"
import { getTranslations } from "next-intl/server"

import { CheckoutForm } from "@/components/checkout-form"

export async function generateMetadata(): Promise<Metadata> {
  return {
    title: (await getTranslations("checkout"))("title"),
    robots: { index: false },
  }
}

export default async function CheckoutPage() {
  const t = await getTranslations("checkout")
  return (
    <div className="flex flex-col gap-6">
      <h1 className="heading-1">{t("title")}</h1>
      <CheckoutForm />
    </div>
  )
}
