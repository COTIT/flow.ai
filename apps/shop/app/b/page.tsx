import type { Metadata } from "next"
import { redirect } from "next/navigation"
import { getTranslations } from "next-intl/server"

import { Notice } from "@genesy/ui/components/notice"

import { PageIntro } from "@/components/page-intro"
import { TraceForm } from "@/components/trace-form"
import { isPublicCode, normalizeCode } from "@/lib/passport"
import { pageMetadata } from "@/lib/seo"
import { getShopBase } from "@/lib/shop-scope"
import { shopPath } from "@/lib/shop-path"

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations()
  const title = t("trace.title")
  if (await getShopBase()) return { title }
  return pageMetadata("/b", { title, description: t("about.traceBody") })
}

/** The trace form's target: a well-formed code goes to its passport, anything else asks again. */
export default async function TracePage({
  searchParams,
}: {
  searchParams: Promise<{ code?: string | string[] }>
}) {
  const raw = (await searchParams).code
  const typed = typeof raw === "string" ? raw : ""
  const code = normalizeCode(typed)
  if (isPublicCode(code)) redirect(shopPath(await getShopBase(), `/b/${code}`))
  const t = await getTranslations()
  return (
    <>
      <PageIntro
        label={t("trace.title")}
        title={t("trace.title")}
        lead={t("about.traceBody")}
      />
      {typed ? (
        <Notice variant="warning" title={t("trace.invalidTitle")}>
          {t("trace.invalidBody")}
        </Notice>
      ) : null}
      <TraceForm value={typed.slice(0, 20)} invalid={Boolean(typed)} />
    </>
  )
}
