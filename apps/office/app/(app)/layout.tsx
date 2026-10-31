"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useLogout, useSession } from "@genesy/api-client/auth"
import { useUsersWorkspaces } from "@genesy/api-client/endpoints/users"
import { sessionStore } from "@genesy/api-client/session"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Button } from "@genesy/ui/components/button"
import { Logo } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { cn } from "@genesy/ui/lib/utils"

const nav = [
  { href: "/products", label: "nav.products" },
  { href: "/stock", label: "nav.stock" },
  { href: "/lots", label: "nav.lots" },
  { href: "/settings/app-keys", label: "nav.appKeys" },
  { href: "/settings/webhooks", label: "nav.webhooks" },
] as const

/** Signed-in frame for every back-office screen: guard, header and menu. */
export default function AppLayout({ children }: { children: React.ReactNode }) {
  const t = useTranslations()
  const router = useRouter()
  const pathname = usePathname()
  const hydrated = useSession((s) => s.hydrated)
  const token = useSession((s) => s.token)
  const workspaceId = useSession((s) => s.workspaceId)
  const logout = useLogout()
  const workspaces = useUsersWorkspaces({ query: { enabled: Boolean(token) } })

  React.useEffect(() => {
    if (hydrated && !token) router.replace("/login")
  }, [hydrated, token, router])

  if (!hydrated || !token) return null

  return (
    <div className="flex min-h-svh flex-col gap-4 p-4 md:p-6">
      <header className="flex flex-wrap items-center gap-3">
        <Logo />
        <p className="mr-auto heading-3 text-muted-foreground">
          {t("app.title")}
        </p>
        <select
          aria-label={t("common.workspace")}
          className="h-8 max-w-48 rounded-lg border border-input bg-background px-2 text-sm"
          value={workspaceId ?? ""}
          onChange={(e) =>
            sessionStore.getState().selectWorkspace(e.target.value)
          }
        >
          {workspaces.data?.data.map((w) => (
            <option key={w.id} value={w.id}>
              {w.workspace_name}
            </option>
          ))}
        </select>
        <LocaleSwitcher />
        <ThemeToggle label={t("common.theme")} />
        <Button
          variant="outline"
          disabled={logout.isPending}
          onClick={() => logout.mutate()}
        >
          {t("common.logout")}
        </Button>
      </header>
      <nav
        aria-label={t("nav.label")}
        className="-mx-1 flex gap-1 overflow-x-auto border-b border-border pb-2"
      >
        {nav.map((item) => (
          <Link
            key={item.href}
            href={item.href}
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
      <main className="flex flex-col gap-4">{children}</main>
    </div>
  )
}
