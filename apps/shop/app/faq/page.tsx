import type { Metadata } from "next"
import { getTranslations } from "next-intl/server"
import { ChevronDownIcon, MailIcon } from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"
import { cn } from "@genesy/ui/lib/utils"

import { PageIntro } from "@/components/page-intro"
import { card, prose } from "@/lib/look"
import { mailto } from "@/lib/nav"

export async function generateMetadata(): Promise<Metadata> {
  return { title: (await getTranslations("faq"))("title") }
}

export default async function FaqPage() {
  const t = await getTranslations()
  const items = t.raw("faq.items") as { q: string; a: string }[]
  return (
    <>
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
      <div className={cn(card, "gap-0 p-2 md:gap-0 md:p-3")}>
        {items.map(({ q, a }) => (
          // Native disclosure: keyboard, screen readers and find-in-page work with no script.
          <details
            key={q}
            className="group border-b border-(--hair) last:border-b-0"
          >
            <summary className="flex min-h-12 cursor-pointer list-none items-center justify-between gap-3 rounded-[12px] px-3 py-3 text-[15px] font-semibold [&::-webkit-details-marker]:hidden">
              {q}
              <ChevronDownIcon
                aria-hidden
                className="size-5 shrink-0 transition-transform group-open:rotate-180 motion-reduce:transition-none"
              />
            </summary>
            <p className={cn(prose, "px-3 pb-4 text-muted-foreground")}>{a}</p>
          </details>
        ))}
      </div>
    </>
  )
}
