"use client"

import { useTranslations } from "next-intl"

import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Logo } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { useTill } from "@/lib/till-store"

/** The top bar before anyone is logged in (setup, login): logo, register, language, theme. */
export function AuthTop() {
  const t = useTranslations("common")
  const register = useTill((s) => s.device?.registerName)
  return (
    <header className="till-top glass-strong">
      <Logo className="h-[22px] min-w-0 shrink-0" />
      <span className="grow" />
      {register ? <span className="who">{register}</span> : null}
      <LocaleSwitcher className="h-11 shrink-0 border-transparent bg-transparent px-3 font-medium" />
      <ThemeToggle label={t("theme")} className="size-11" />
    </header>
  )
}
