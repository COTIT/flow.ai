import Link from "@/components/shop-link"
import { getTranslations } from "next-intl/server"
import { MailIcon } from "lucide-react"

import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { LogoIcon } from "@genesy/ui/components/logo"

import { NAV, mailto } from "@/lib/nav"

const link =
  "inline-flex min-h-11 items-center rounded-sm underline-offset-4 hover:underline md:min-h-8"

/** Navy card (a quiet glass edge in dark mode): who we are, links, newsletter by email. */
export async function SiteFooter({ shopName }: { shopName?: string }) {
  const t = await getTranslations()
  if (shopName)
    return (
      <footer className="flex flex-wrap items-center justify-between gap-4 rounded-[24px] bg-primary p-7 text-primary-foreground">
        <p className="font-semibold">{shopName}</p>
        <Link href="/shop" className={link}>
          {t("nav.shop")}
        </Link>
        <p className="flex items-center gap-2 text-xs">
          <LogoIcon aria-hidden className="h-auto w-5" />
          {t("footer.tracedWith")}
        </p>
      </footer>
    )
  const email = t("content.email")
  return (
    <footer className="grid gap-8 rounded-[24px] bg-primary p-7 text-primary-foreground md:grid-cols-[1.4fr_1fr_1.4fr] md:p-10 dark:border dark:border-(--hair) dark:bg-white/4 dark:text-foreground">
      <div className="flex flex-col gap-4">
        <p className="text-[20px] font-semibold">{t("app.title")}</p>
        <p className="text-[18px] leading-snug font-medium">
          {t("footer.title")}
        </p>
        <p className="text-[13px] opacity-80">{t("footer.shipping")}</p>
        <p className="flex items-center gap-2 text-[12px] opacity-70">
          <LogoIcon aria-hidden className="h-auto w-5" />
          {t("footer.tracedWith")}
        </p>
      </div>
      <nav aria-label={t("footer.explore")} className="flex flex-col">
        <p className="mb-1 text-[11px] font-extrabold tracking-[0.08em] uppercase opacity-70">
          {t("footer.explore")}
        </p>
        {NAV.map(({ href, key }) => (
          <Link key={key} href={href} className={link}>
            {t(`nav.${key}`)}
          </Link>
        ))}
        <Link href="/suppliers" className={link}>
          {t("footer.supplier")}
        </Link>
      </nav>
      <div className="flex flex-col items-start gap-3">
        <p className="text-[11px] font-extrabold tracking-[0.08em] uppercase opacity-70">
          {t("footer.newsletterTitle")}
        </p>
        <p className="text-[13px] opacity-80">{t("footer.newsletterBody")}</p>
        <a
          href={mailto(email, t("footer.newsletterSubject"))}
          className="inline-flex min-h-11 items-center gap-2 rounded-full bg-(image:--gold-gradient) px-4 text-[14px] font-semibold text-[#0a183a]"
        >
          <MailIcon aria-hidden className="size-4" />
          {t("footer.newsletterCta")}
        </a>
        <LocaleSwitcher className="mt-2 h-11 border-white/30 bg-transparent text-[13px] text-inherit md:h-10 [&>option]:text-[#0a183a]" />
      </div>
    </footer>
  )
}
