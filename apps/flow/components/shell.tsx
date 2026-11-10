"use client"

import { useTranslations } from "next-intl"
import { LogOut } from "lucide-react"

import { useLogout, useSession } from "@genesy/api-client/auth"
import { useUsersWorkspaces } from "@genesy/api-client/endpoints/users"
import { sessionStore } from "@genesy/api-client/session"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Button } from "@genesy/ui/components/button"
import { LogoIcon } from "@genesy/ui/components/logo"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@genesy/ui/components/select"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"

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
  return (
    <div className="flex min-h-svh flex-col gap-5 p-5 max-md:gap-3 max-md:p-3">
      <header className="glass-strong flex flex-wrap items-center gap-2 rounded-2xl px-4 py-2.5">
        <LogoIcon aria-hidden="true" className="size-7 shrink-0" />
        <span className="mr-auto text-[15px] font-semibold">
          {t("flow.name")}
        </span>
        <WorkspacePicker />
        <LocaleSwitcher />
        <ThemeToggle label={t("common.theme")} />
        <Button
          variant="ghost"
          size="sm"
          disabled={logout.isPending}
          onClick={() => logout.mutate()}
        >
          <LogOut aria-hidden="true" />
          {t("common.logout")}
        </Button>
      </header>
      <main className="mx-auto flex w-full max-w-2xl min-w-0 flex-col gap-5 max-md:gap-3">
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
