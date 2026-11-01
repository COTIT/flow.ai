"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useTillLogout } from "@genesy/api-client/endpoints/till"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Button } from "@genesy/ui/components/button"
import { Logo } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { tillStore, useTill } from "@/lib/till-store"

/** Logged-in frame for the till screens: guard, register and user, logout. */
export default function TillLayout({ children }: { children: React.ReactNode }) {
  const t = useTranslations()
  const router = useRouter()
  const hydrated = useTill((s) => s.hydrated)
  const device = useTill((s) => s.device)
  const session = useTill((s) => s.session)
  const logout = useTillLogout({
    mutation: { onSettled: () => tillStore.getState().signOut() },
  })

  React.useEffect(() => {
    if (!hydrated) return
    if (!device) router.replace("/setup")
    else if (!session) router.replace("/login")
  }, [hydrated, device, session, router])

  if (!hydrated || !device || !session) return null

  return (
    <div className="flex min-h-svh flex-col gap-4 p-4 md:p-6">
      <header className="flex flex-wrap items-center gap-3">
        <Logo />
        <p className="mr-auto text-sm text-muted-foreground">
          <span className="font-medium text-foreground">
            {session.register.name}
          </span>
          {" · "}
          {session.user.name}
        </p>
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
      <main className="flex flex-col gap-4">{children}</main>
    </div>
  )
}
