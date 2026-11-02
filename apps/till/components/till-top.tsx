"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import { useTranslations } from "next-intl"

import { useTillLogout } from "@genesy/api-client/endpoints/till"
import { Button } from "@genesy/ui/components/button"
import { Logo } from "@genesy/ui/components/logo"
import { QueueRunner } from "@/components/queue-runner"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { tillStore, useTill } from "@/lib/till-store"

const nav = [
  { href: "/sale", label: "nav.sale" },
  { href: "/drawer", label: "nav.drawer" },
] as const

/** Lock: ends the till user's session (the device stays paired), back to the PIN screen. */
export function useLock() {
  const device = useTill((s) => s.device)
  const logout = useTillLogout({
    mutation: { onSettled: () => tillStore.getState().signOut() },
  })
  return {
    pending: logout.isPending,
    lock: () => device && logout.mutate({ workspace: device.workspaceId }),
  }
}

/** The signed-in top bar (TillSale): logo, Sale/Drawer, status, register · user, theme, Lock. */
export function TillTop() {
  const t = useTranslations()
  const pathname = usePathname()
  const session = useTill((s) => s.session)
  const { lock, pending } = useLock()
  return (
    <header className="till-top glass-strong">
      <Logo className="h-[22px] min-w-0 shrink-0" />
      <nav aria-label={t("nav.label")} className="flex gap-1">
        {nav.map((item) => (
          <Link
            key={item.href}
            href={item.href}
            // No prefetch: a till runs on flaky networks and has two screens.
            prefetch={false}
            aria-current={pathname.startsWith(item.href) ? "page" : undefined}
            className="till-tab"
          >
            {t(item.label)}
          </Link>
        ))}
      </nav>
      <span className="grow" />
      <QueueRunner />
      <span className="who no-phone">
        {session?.register.name} · {session?.user.name}
      </span>
      <ThemeToggle label={t("common.theme")} className="size-11" />
      <Button
        variant="outline"
        className="btn-44"
        disabled={pending}
        onClick={lock}
      >
        {t("nav.lock")}
      </Button>
    </header>
  )
}
