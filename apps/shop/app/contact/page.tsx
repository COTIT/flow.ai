import type { Metadata } from "next"
import Link from "next/link"
import { getTranslations } from "next-intl/server"
import { HandshakeIcon, MailIcon, MapPinIcon } from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { cn } from "@genesy/ui/lib/utils"

import { PageIntro } from "@/components/page-intro"
import { card, goldChip, h2 } from "@/lib/look"
import { mailto } from "@/lib/nav"

export async function generateMetadata(): Promise<Metadata> {
  return { title: (await getTranslations("contact"))("tag") }
}

const chip = cn(goldChip, "size-12 rounded-[14px]")

export default async function ContactPage() {
  const t = await getTranslations()
  const email = t("content.email")
  return (
    <>
      <PageIntro
        label={t("contact.tag")}
        title={t("contact.title")}
        lead={t("contact.lead")}
      />
      <section className="grid gap-3 md:grid-cols-3 md:gap-5">
        <article className={card}>
          <span className={chip}>
            <MailIcon aria-hidden className="size-6" />
          </span>
          <h2 className={h2}>{t("contact.emailTitle")}</h2>
          <a
            href={mailto(email, t("contact.title"))}
            className="inline-flex min-h-11 items-center font-semibold break-all underline underline-offset-4"
          >
            {email}
          </a>
          <p className="text-muted-foreground">{t("contact.emailBody")}</p>
        </article>
        <article className={card}>
          <span className={chip}>
            <MapPinIcon aria-hidden className="size-6" />
          </span>
          <h2 className={h2}>{t("contact.whereTitle")}</h2>
          <p>{t("contact.us")}</p>
          <p>{t("contact.farm")}</p>
        </article>
        <article className={card}>
          <span className={chip}>
            <HandshakeIcon aria-hidden className="size-6" />
          </span>
          <h2 className={h2}>{t("contact.supplierTitle")}</h2>
          <p className="text-muted-foreground">{t("contact.supplierBody")}</p>
          <Link
            href="/suppliers"
            data-slot="button"
            className={cn(buttonVariants({ variant: "outline" }), "self-start")}
          >
            {t("footer.supplier")}
          </Link>
        </article>
      </section>
      <Notice variant="info" title={t("contact.formNote")} live={false} />
    </>
  )
}
