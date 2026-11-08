"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"
import {
  Archive,
  Calculator,
  ChevronRight,
  FileText,
  Globe,
  LayoutGrid,
  KeyRound,
  Layers,
  LogOut,
  Menu,
  Mic,
  Package,
  Tags,
  Search,
  ShoppingBag,
  Truck,
  UserPlus,
  Zap,
  type LucideIcon,
} from "lucide-react"

import { useLogout, useSession } from "@genesy/api-client/auth"
import { useOrdersList } from "@genesy/api-client/endpoints/orders"
import { useSupplyLinksList } from "@genesy/api-client/endpoints/supply-links"
import {
  useUsersMe,
  useUsersWorkspaces,
} from "@genesy/api-client/endpoints/users"
import { sessionStore } from "@genesy/api-client/session"
import { LOCALE_COOKIE, locales } from "@genesy/i18n/locales"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@genesy/ui/components/dropdown-menu"
import { Input } from "@genesy/ui/components/input"
import { Logo } from "@genesy/ui/components/logo"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@genesy/ui/components/select"
import { Sheet, SheetContent, SheetTitle } from "@genesy/ui/components/sheet"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { cn } from "@genesy/ui/lib/utils"

import { VoiceControls } from "@/components/voice/controls"
import { rightFor, useCan, useRights } from "@/lib/can"

type Item = {
  href: string
  /** Every path under this prefix marks the item current. */
  match: string
  label: string
  icon: LucideIcon
  /** Group heading shown above the item (OfficeOverview sidebar). */
  group?: string
  /** A gold pill with a list's total: open orders, supply links waiting for us. */
  count?: "orders" | "supply"
}

const items: Item[] = [
  { href: "/", match: "/", label: "nav.overview", icon: LayoutGrid },
  { href: "/voice", match: "/voice", label: "nav.voice", icon: Mic },
  {
    href: "/products",
    match: "/products",
    label: "nav.products",
    icon: Package,
    group: "shell.sell",
  },
  {
    href: "/products/categories",
    match: "/products/categories",
    label: "nav.categories",
    icon: Tags,
  },
  { href: "/stock", match: "/stock", label: "nav.stock", icon: Archive },
  {
    href: "/orders",
    match: "/orders",
    label: "nav.orders",
    icon: ShoppingBag,
    count: "orders",
  },
  {
    href: "/invoices",
    match: "/invoices",
    label: "nav.invoices",
    icon: FileText,
  },
  {
    href: "/lots",
    match: "/lots",
    label: "nav.lots",
    icon: Layers,
    group: "shell.trace",
  },
  {
    href: "/supply/links",
    match: "/supply",
    label: "nav.supply",
    icon: Truck,
    count: "supply",
  },
  {
    href: "/till/registers",
    match: "/till",
    label: "nav.till",
    icon: Calculator,
    group: "shell.store",
  },
  {
    href: "/settings/app-keys",
    match: "/settings/app-keys",
    label: "nav.appKeys",
    icon: KeyRound,
    group: "shell.settings",
  },
  {
    href: "/settings/webhooks",
    match: "/settings/webhooks",
    label: "nav.webhooks",
    icon: Zap,
  },
  {
    href: "/settings/invites",
    match: "/settings/invites",
    label: "nav.invites",
    icon: UserPlus,
  },
]

// Each entry with the heading of the section it sits in (the last group named at or above it).
const sectioned = items.map((item, i) => ({
  ...item,
  group: items
    .slice(0, i + 1)
    .reverse()
    .find((x) => x.group)?.group,
}))

// Last match wins: /products/categories is Categories, not Products.
const current = (pathname: string) =>
  [...items]
    .reverse()
    .find(
      (i) =>
        pathname === i.match ||
        (i.match !== "/" && pathname.startsWith(`${i.match}/`))
    )

/** Initials on the gold disc (sidebar foot, phone bar). */
export function Avatar({ initials }: { initials?: string }) {
  return (
    <span
      aria-hidden="true"
      className="inline-flex size-9 shrink-0 items-center justify-center rounded-full bg-[image:var(--gold-gradient)] text-[13px] font-semibold text-[#0a183a]"
    >
      {initials}
    </span>
  )
}

function NavList({ onNavigate }: { onNavigate?: () => void }) {
  const t = useTranslations()
  const pathname = usePathname()
  const workspaceId = useSession((s) => s.workspaceId)
  // The menu counts, read from the lists' totals (no sums in the browser), and only those the
  // member may read: the API answers 403 to the rest.
  const on = (allowed: boolean) => ({
    query: { enabled: Boolean(workspaceId) && allowed },
  })
  const open = useOrdersList(
    workspaceId ?? "",
    { status: "open", page_size: 1 },
    on(useCan("order.list"))
  )
  const pending = useSupplyLinksList(
    workspaceId ?? "",
    { status: "pending", direction: "incoming", page_size: 1 },
    on(useCan("workspace_admin"))
  )
  const counts = {
    orders: open.data?.meta.total ?? 0,
    supply: pending.data?.meta.total ?? 0,
  }
  const active = current(pathname)
  // Only the entries the member may use (all of them until /me answers, so nothing jumps for
  // most). A group's heading goes with its first entry left.
  const { read, may } = useRights()
  const shown = sectioned.filter((item) => {
    const right = rightFor(item.href)
    return !read || !right || may(right)
  })
  return (
    <nav aria-label={t("nav.label")} className="flex flex-col gap-0.5">
      {shown.map((item, i) => (
        <React.Fragment key={item.href}>
          {item.group && item.group !== shown[i - 1]?.group ? (
            <p className="px-2.5 pt-3 pb-1.5 text-[11px] font-bold tracking-[0.08em] text-[#6b7f9c] uppercase dark:text-[#8fa6c8]">
              {t(item.group)}
            </p>
          ) : null}
          <Link
            href={
              // The assistant suggests things for the page it was opened from.
              item.href === "/voice" && pathname !== "/voice"
                ? `/voice?from=${encodeURIComponent(pathname)}`
                : item.href
            }
            onClick={onNavigate}
            aria-current={item.href === active?.href ? "page" : undefined}
            className="group flex h-10 items-center gap-3 rounded-xl px-3 text-sm font-medium text-foreground outline-none hover:bg-(--glass-subtle) focus-visible:ring-3 focus-visible:ring-ring aria-[current=page]:bg-primary aria-[current=page]:text-primary-foreground aria-[current=page]:shadow-[0_8px_18px_-10px_rgb(10_24_58/60%)] max-md:h-11"
          >
            <item.icon
              aria-hidden="true"
              strokeWidth={1.75}
              className="size-[18px] shrink-0 text-muted-foreground group-aria-[current=page]:text-gold dark:group-aria-[current=page]:text-[#0a183a]"
            />
            {t(item.label)}
            {item.count && counts[item.count] > 0 ? (
              <span className="ml-auto inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-[image:var(--gold-gradient)] px-1.5 text-[11px] font-bold text-[#0a183a] tabular-nums">
                {counts[item.count]}
              </span>
            ) : null}
          </Link>
        </React.Fragment>
      ))}
    </nav>
  )
}

function useWorkspaces() {
  const token = useSession((s) => s.token)
  return useUsersWorkspaces({ query: { enabled: Boolean(token) } })
}

function UserTile() {
  const t = useTranslations()
  const me = useUsersMe()
  const user = me.data?.data.user
  const workspaceId = useSession((s) => s.workspaceId)
  const workspace = useWorkspaces().data?.data.find((w) => w.id === workspaceId)
  const logout = useLogout()
  return (
    <div className="mt-auto flex items-center gap-2.5 rounded-2xl border border-(--hair) bg-(--glass-subtle) p-3">
      <Avatar initials={user?.initials} />
      <div className="min-w-0 flex-1">
        {/* min-h: the lines keep their height while the name loads. */}
        <p className="min-h-[1lh] truncate text-[13px] font-semibold">
          {user?.name}
        </p>
        <p className="min-h-[1lh] truncate text-xs text-muted-foreground">
          {workspace?.display_name}
        </p>
      </div>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t("common.logout")}
        disabled={logout.isPending}
        onClick={() => logout.mutate()}
      >
        <LogOut aria-hidden="true" />
      </Button>
    </div>
  )
}

function WorkspacePicker({ className }: { className?: string }) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId)
  const list = useWorkspaces().data?.data ?? []
  const options = list.map((w) => ({ value: w.id, label: w.display_name }))
  return (
    <Select
      items={options}
      // Until the list loads there's no name to show: no value, so the raw id never flashes.
      value={options.length ? (workspaceId ?? null) : null}
      onValueChange={(v) => {
        if (v) sessionStore.getState().selectWorkspace(v)
      }}
    >
      <SelectTrigger aria-label={t("common.workspace")} className={className}>
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

/** Language picker: the same cookie as @genesy/i18n's LocaleSwitcher, drawn as a menu. */
export function LocaleMenu({ withName = false }: { withName?: boolean }) {
  const t = useTranslations("common")
  const locale = useLocale()
  const router = useRouter()
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size={withName ? "sm" : "icon"}
            aria-label={t("language")}
          />
        }
      >
        {withName ? (
          <>
            <Globe aria-hidden="true" />
            {t(`languages.${locale}`)}
          </>
        ) : (
          locale.toUpperCase()
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuRadioGroup
          value={locale}
          onValueChange={(value: string) => {
            document.cookie = `${LOCALE_COOKIE}=${value}; path=/; max-age=31536000; samesite=lax`
            router.refresh()
          }}
        >
          {locales.map((l) => (
            <DropdownMenuRadioItem key={l} value={l}>
              {t(`languages.${l}`)}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/** The quick search is the API's global search: offered while /me loads, then only with the right. */
function useSearchAllowed() {
  const { read, may } = useRights()
  return !read || may("workspace.global-search")
}

function TopBar() {
  const t = useTranslations()
  const search = useSearchAllowed()
  const router = useRouter()
  const pathname = usePathname()
  const workspaceId = useSession((s) => s.workspaceId)
  const workspace = useWorkspaces().data?.data.find((w) => w.id === workspaceId)
  const active = current(pathname)
  return (
    <div
      data-topbar=""
      className="hidden items-center gap-3 rounded-[20px] glass-strong py-2.5 pr-3 pl-[18px] md:flex"
    >
      {/* Drawn once the workspace name is known, so the page name never slides along. */}
      {/* Below xl the sidebar already marks the page, and the bar has no room for the crumb. */}
      <p className="flex min-w-0 flex-1 items-center gap-1.5 text-[13px] text-muted-foreground max-xl:hidden">
        {workspace ? (
          <>
            <span className="min-w-0 truncate">{workspace.display_name}</span>
            <ChevronRight
              aria-hidden="true"
              className="size-4 shrink-0"
              strokeWidth={1.75}
            />
            <b className="font-semibold whitespace-nowrap text-foreground">
              {active ? t(active.label) : null}
            </b>
          </>
        ) : null}
      </p>
      {/* Anchored right with a fixed width, so names loading on the left never move it. Kept in
          place but invisible (out of reach) for a member who may not search. */}
      <form
        role="search"
        className={cn(
          "relative w-[340px] shrink-0 max-xl:w-auto max-xl:min-w-0 max-xl:flex-1 min-[1400px]:w-[420px]",
          !search && "invisible"
        )}
        onSubmit={(e) => {
          e.preventDefault()
          const q = String(new FormData(e.currentTarget).get("q") ?? "")
          router.push(`/search?${new URLSearchParams(q ? { q } : {})}`)
        }}
      >
        <Search
          aria-hidden="true"
          strokeWidth={1.75}
          className="pointer-events-none absolute top-1/2 left-3.5 size-[18px] -translate-y-1/2 text-muted-foreground"
        />
        <Input
          name="q"
          type="search"
          aria-label={t("shell.search")}
          placeholder={t("shell.searchPlaceholder")}
          className="h-10 pl-10"
        />
      </form>
      <WorkspacePicker className="w-[200px] max-lg:w-[160px]" />
      <LocaleMenu />
      <ThemeToggle label={t("common.theme")} />
    </div>
  )
}

function MobileBar({ onMenu }: { onMenu: () => void }) {
  const t = useTranslations()
  const user = useUsersMe().data?.data.user
  const search = useSearchAllowed()
  return (
    <div className="flex items-center gap-2.5 rounded-[18px] glass-strong py-2 pr-2 pl-3.5 md:hidden">
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("shell.openMenu")}
        onClick={onMenu}
      >
        <Menu aria-hidden="true" />
      </Button>
      <Logo className="h-auto w-[112px]" />
      <span className="flex-1" />
      {search ? (
        <Link
          href="/search"
          aria-label={t("shell.search")}
          className={buttonVariants({ variant: "ghost", size: "icon" })}
        >
          <Search aria-hidden="true" />
        </Link>
      ) : null}
      <Avatar initials={user?.initials} />
    </div>
  )
}

/**
 * The signed-in frame (OfficeOverview): glass sidebar and top bar on a desktop; on a phone a
 * slim bar whose menu button slides the same sidebar in as a sheet. Switched by CSS, so the
 * first paint is already right (no layout shift).
 */
export function AppShell({ children }: { children: React.ReactNode }) {
  const t = useTranslations()
  const [menu, setMenu] = React.useState(false)
  const pathname = usePathname()
  return (
    <div className="flex min-h-svh items-start gap-5 p-5 max-md:flex-col max-md:items-stretch max-md:gap-3 max-md:p-3">
      <aside className="sticky top-5 hidden max-h-[calc(100svh-40px)] w-[252px] shrink-0 flex-col gap-[18px] overflow-y-auto rounded-[24px] glass-strong px-3.5 py-[18px] md:flex">
        <div className="px-2 pt-1 pb-1.5">
          <Logo className="h-auto w-[150px]" />
        </div>
        <NavList />
        <UserTile />
      </aside>
      <div className="flex min-w-0 flex-1 flex-col gap-5 max-md:gap-3">
        <MobileBar onMenu={() => setMenu(true)} />
        <TopBar />
        <div className="rounded-2xl glass-strong px-3 py-2 empty:hidden">
          {pathname !== "/voice" ? <VoiceControls compact /> : null}
        </div>
        <main className="flex min-w-0 flex-col gap-5 max-md:gap-3">
          {children}
        </main>
      </div>
      <Sheet open={menu} onOpenChange={setMenu}>
        <SheetContent
          side="left"
          className={cn(
            "w-[288px] gap-[18px] overflow-y-auto px-3.5 py-[18px] md:hidden"
          )}
        >
          <SheetTitle className="sr-only">{t("nav.label")}</SheetTitle>
          <div className="px-2 pt-1 pb-1.5">
            <Logo className="h-auto w-[150px]" />
          </div>
          <NavList onNavigate={() => setMenu(false)} />
          <div className="flex flex-col gap-2">
            <WorkspacePicker className="w-full" />
            <div className="flex items-center gap-2">
              <LocaleMenu withName />
              <ThemeToggle label={t("common.theme")} />
            </div>
          </div>
          <UserTile />
        </SheetContent>
      </Sheet>
    </div>
  )
}
