import type { Metadata } from "next"
import { getLocale, getTranslations } from "next-intl/server"

import { PassportView } from "@genesy/ui/components/passport-view"

import { getLot } from "@/lib/storefront"

type Props = { params: Promise<{ code: string }> }

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const lot = await getLot((await params).code)
  const title = lot.passport.title ?? lot.product.name
  const description =
    lot.passport.intro?.slice(0, 160) ??
    lot.product.short_description ??
    undefined
  const url = `/b/${lot.public_code}`
  // No og:image: banner links are signed and expire within minutes.
  return {
    title,
    description,
    alternates: { canonical: url },
    openGraph: { title, description, url, type: "article" },
  }
}

/** What a jar's QR code opens. Unknown or unpublished codes get a real 404 from getLot. */
export default async function BatchPage({ params }: Props) {
  const { code } = await params
  const [t, locale, lot] = await Promise.all([
    getTranslations("batch"),
    getLocale(),
    getLot(code),
  ])
  return (
    <PassportView passport={lot.passport} lot={lot} locale={locale} t={t} />
  )
}
