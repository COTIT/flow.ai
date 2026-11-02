"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { tillMe, useTillLogout } from "@genesy/api-client/endpoints/till"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Button } from "@genesy/ui/components/button"
import { Logo } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { cn } from "@genesy/ui/lib/utils"
import { QueueRunner } from "@/components/queue-runner"
import { Loading, Notice } from "@/components/state"
import { pendingCount, useQueue } from "@/lib/sale-queue"
import { tillStore, useTill } from "@/lib/till-store"
import { useOnline } from "@/lib/use-online"

const nav = [
  { href: "/sale", label: "nav.sale" },
  { href: "/drawer", label: "nav.drawer" },
] as const

/** Logged-in frame for the till screens: guard, register and user, logout. */
export default function TillLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const t = useTranslations()
  const router = useRouter()
  const pathname = usePathname()
  const hydrated = useTill((s) => s.hydrated)
  const device = useTill((s) => s.device)
  const session = useTill((s) => s.session)
  const online = useOnline()
  const waiting = useQueue(pendingCount)
  const logout = useTillLogout({
    mutation: { onSettled: () => tillStore.getState().signOut() },
  })

  React.useEffect(() => {
    if (!hydrated) return
    if (!device) router.replace("/setup")
    else if (!session) router.replace("/login")
  }, [hydrated, device, session, router])

  // The stored login may predate a field (currency, accepted cash) or a settings change:
  // refresh the register from /till/me on start and whenever the till comes back online.
  const token = session?.token
  React.useEffect(() => {
    if (!token || !device) return
    const refresh = () =>
      tillMe(device.workspaceId)
        .then(({ data }) => {
          const now = tillStore.getState().session
          if (now?.token === token)
            tillStore.getState().signIn({ ...now, ...data })
        })
        .catch(() => undefined) // offline or ended: the stored copy stands, a 401 logs out
    void refresh()
    window.addEventListener("online", refresh)
    return () => window.removeEventListener("online", refresh)
  }, [token, device])

  if (!hydrated || !device || !session)
    return (
      <main className="p-6">
        <Loading>{t("common.loading")}</Loading>
      </main>
    )

  return (
    <div className="flex min-h-svh flex-col gap-4 p-4 md:p-6">
      <header className="flex flex-wrap items-center gap-3 print:hidden">
        <Logo />
        <p className="mr-auto text-sm text-muted-foreground">
          <span className="font-medium text-foreground">
            {session.register.name}
          </span>
          {" · "}
          {session.user.name}
        </p>
        <QueueRunner />
        <LocaleSwitcher />
        <ThemeToggle label={t("common.theme")} />
        <Button
          variant="outline"
          disabled={logout.isPending}
          onClick={() => logout.mutate({ workspace: device.workspaceId })}
        >
          {t("common.logout")}
        </Button>
      </header>
      <nav
        aria-label={t("nav.label")}
        className="-mx-1 flex gap-1 overflow-x-auto border-b border-border pb-2 print:hidden"
      >
        {nav.map((item) => (
          <Link
            key={item.href}
            href={item.href}
            // No prefetch: a till runs on flaky networks and has two screens.
            prefetch={false}
            aria-current={pathname.startsWith(item.href) ? "page" : undefined}
            className={cn(
              "shrink-0 rounded-lg px-3 py-1.5 text-sm text-muted-foreground hover:bg-muted",
              "aria-[current=page]:bg-secondary aria-[current=page]:font-medium aria-[current=page]:text-foreground"
            )}
          >
            {t(item.label)}
          </Link>
        ))}
      </nav>
      {online ? null : (
        <Notice kind="warning" className="print:hidden">
          <p>{t("offline.title")}</p>
          {waiting ? (
            <Link href="/queue" prefetch={false} className="underline">
              {t("offline.waiting", { count: waiting })}
            </Link>
          ) : null}
        </Notice>
      )}
      <main className="flex flex-col gap-4">{children}</main>
    </div>
  )
}
