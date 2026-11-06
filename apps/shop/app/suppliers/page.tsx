import type { Metadata } from "next"
import { getTranslations } from "next-intl/server"
import { MailIcon } from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"
import { cn } from "@genesy/ui/lib/utils"

import { PageIntro } from "@/components/page-intro"
import { card, h2, prose, tile } from "@/lib/look"
import { mailto } from "@/lib/nav"

export async function generateMetadata(): Promise<Metadata> {
  return { title: (await getTranslations("suppliers"))("tag") }
}

export default async function SuppliersPage() {
  const t = await getTranslations()
  return (
    <>
      <PageIntro
        label={t("suppliers.tag")}
        title={t("suppliers.title")}
        lead={t("suppliers.lead")}
      />
      <section className="grid gap-3 md:grid-cols-2 md:gap-5">
        <article className={card}>
          <h2 className={h2}>{t("suppliers.whoTitle")}</h2>
          <ul className="flex list-disc flex-col gap-1.5 pl-5">
            {(t.raw("suppliers.who") as string[]).map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </article>
        <article className={card}>
          <h2 className={h2}>{t("suppliers.howTitle")}</h2>
          <ol className="flex list-decimal flex-col gap-1.5 pl-5">
            {(t.raw("suppliers.how") as string[]).map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ol>
        </article>
      </section>
      <section lang="sw" className={cn(card, tile)}>
        <h2 className={h2}>{t("suppliers.swTitle")}</h2>
        <p className={prose}>{t("suppliers.swBody")}</p>
      </section>
      <section
        className={cn(card, "items-start gap-3 md:flex-row md:items-center")}
      >
        <div className="flex-1">
          <h2 className={h2}>{t("suppliers.ctaTitle")}</h2>
          <p className="text-muted-foreground">{t("suppliers.ctaBody")}</p>
        </div>
        <a
          href={mailto(t("content.email"), t("suppliers.subject"))}
          data-slot="button"
          className={buttonVariants({ size: "lg" })}
        >
          <MailIcon aria-hidden />
          {t("content.emailUs")}
        </a>
      </section>
    </>
  )
}
