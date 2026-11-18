import type { Metadata } from "next"
import { getTranslations } from "next-intl/server"
import { MinusIcon, PlusIcon, MailIcon } from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"
import { cn } from "@genesy/ui/lib/utils"

import { PageIntro } from "@/components/page-intro"
import { pageMetadata } from "@/lib/seo"
import { card, prose } from "@/lib/look"
import { mailto } from "@/lib/nav"

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations("faq")
  return pageMetadata("/faq", { title: t("title"), description: t("lead") })
}

export default async function FaqPage() {
  const t = await getTranslations()
  const items = t.raw("faq.items") as { q: string; a: string }[]
  return (
    <div className="afm-faq contents">
      <PageIntro
        label={t("faq.tag")}
        title={t("faq.title")}
        lead={t("faq.lead")}
      >
        <a
          href={mailto(t("content.email"), t("faq.title"))}
          data-slot="button"
          className={cn(buttonVariants({ variant: "outline" }), "self-start")}
        >
          <MailIcon aria-hidden />
          {t("content.emailUs")}
        </a>
      </PageIntro>
      <div className={cn(card, "afm-faq-list gap-0 p-2 md:gap-0 md:p-3")}>
        {items.map(({ q, a }) => (
          // Native disclosure: keyboard, screen readers and find-in-page work with no script.
          <details
            key={q}
            className="group border-b border-(--hair) last:border-b-0"
          >
            <summary className="flex min-h-12 cursor-pointer list-none items-center justify-between gap-3 rounded-[12px] px-3 py-3 text-[15px] font-semibold [&::-webkit-details-marker]:hidden">
              {q}
              <span
                aria-hidden
                className="afm-faq-toggle grid size-8 shrink-0 place-items-center rounded-full"
              >
                <PlusIcon className="size-4 group-open:hidden" />
                <MinusIcon className="hidden size-4 group-open:block" />
              </span>
            </summary>
            <p className={cn(prose, "px-3 pb-4 text-muted-foreground")}>{a}</p>
          </details>
        ))}
      </div>
    </div>
  )
}
