"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import { useState } from "react"
import { useTranslations } from "next-intl"
import { LogOut } from "lucide-react"

import { useLogout, useSession } from "@genesy/api-client/auth"
import { useUsersWorkspaces } from "@genesy/api-client/endpoints/users"
import { sessionStore } from "@genesy/api-client/session"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { LogoIcon } from "@genesy/ui/components/logo"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@genesy/ui/components/select"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { useVoice } from "@genesy/voice/components/provider"
import { settleWithin } from "@/lib/settle"

/** How long logout waits for the voice session to end with the still-valid login. */
const END_VOICE_MS = 3000

/** The workspaces this login may switch to: the API's list, limited to the login's own ids. */
export function useWorkspaceChoices() {
  const token = useSession((s) => s.token)
  const ids = useSession((s) => s.workspaceIds)
  const query = useUsersWorkspaces({ query: { enabled: Boolean(token) } })
  const choices = (query.data?.data ?? []).filter((w) => ids.includes(w.id))
  return { query, choices }
}

/** One top bar, then the page: no sidebar, the assistant is the whole app. */
export function Shell({ children }: { children: React.ReactNode }) {
  const t = useTranslations()
  const logout = useLogout()
  const voice = useVoice()
  const pathname = usePathname()
  const [leaving, setLeaving] = useState(false)
  // The conversation fills the screen in two panes; every other page keeps the narrow column.
  const wide = pathname === "/"
  return (
    <div
      className={`flex flex-col gap-5 p-5 max-md:gap-3 max-md:p-3 ${wide ? "h-dvh" : "min-h-svh"}`}
    >
      <header className="flex shrink-0 flex-wrap items-center gap-2 rounded-2xl glass-strong px-4 py-2.5">
        <Link href="/" className="mr-auto flex min-h-11 items-center gap-2">
          <LogoIcon aria-hidden="true" className="size-7 shrink-0" />
          <span className="text-[15px] font-semibold">{t("flow.name")}</span>
        </Link>
        <Link
          href="/storefront"
          prefetch={false}
          aria-current={pathname === "/storefront" ? "page" : undefined}
          className={buttonVariants({ variant: "ghost", size: "sm" })}
        >
          {t("flow.chat.storefront")}
        </Link>
        <WorkspacePicker />
        <LocaleSwitcher />
        <ThemeToggle label={t("common.theme")} />
        <Button
          variant="ghost"
          size="sm"
          disabled={leaving || logout.isPending}
          onClick={async () => {
            setLeaving(true)
            // End the voice session while the login still works; a hung end never blocks logout.
            await settleWithin(voice.end(), END_VOICE_MS)
            logout.mutate()
          }}
        >
          <LogOut aria-hidden="true" />
          {t("common.logout")}
        </Button>
      </header>
      <main
        className={`mx-auto flex w-full min-w-0 flex-col gap-5 max-md:gap-3 ${wide ? "min-h-0 max-w-screen-2xl flex-1" : "max-w-2xl"}`}
      >
        {children}
      </main>
    </div>
  )
}

/** Shown only with more than one workspace to choose from. */
function WorkspacePicker() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId)
  const { choices } = useWorkspaceChoices()
  if (choices.length < 2) return null
  const options = choices.map((w) => ({ value: w.id, label: w.display_name }))
  return (
    <Select
      items={options}
      value={workspaceId}
      onValueChange={(v) => {
        if (v) sessionStore.getState().selectWorkspace(v)
      }}
    >
      <SelectTrigger aria-label={t("common.workspace")} className="max-w-48">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
