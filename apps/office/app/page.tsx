"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useLogout, useSession } from "@genesy/api-client/auth"
import { useUsersWorkspaces } from "@genesy/api-client/endpoints/users"
import { useProductsList } from "@genesy/api-client/endpoints/products"
import { sessionStore } from "@genesy/api-client/session"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Button } from "@genesy/ui/components/button"
import { Logo } from "@genesy/ui/components/logo"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"

export default function ProductsPage() {
  const t = useTranslations()
  const router = useRouter()
  const hydrated = useSession((s) => s.hydrated)
  const token = useSession((s) => s.token)
  const workspaceId = useSession((s) => s.workspaceId)
  const logout = useLogout()
  const workspaces = useUsersWorkspaces({ query: { enabled: Boolean(token) } })
  const products = useProductsList(workspaceId ?? "", undefined, {
    query: { enabled: Boolean(token && workspaceId) },
  })

  React.useEffect(() => {
    if (hydrated && !token) router.replace("/login")
  }, [hydrated, token, router])

  if (!hydrated || !token) return null

  return (
    <main className="flex min-h-svh flex-col gap-4 p-4 md:p-6">
      <header className="flex flex-wrap items-center gap-3">
        <Logo />
        <h1 className="mr-auto heading-3 text-muted-foreground">
          {t("app.title")}
        </h1>
        <select
          aria-label={t("common.workspace")}
          className="h-8 rounded-lg border border-input bg-background px-2 text-sm"
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
      <div className="gold-rule" />

      <section className="flex flex-col gap-2">
        <h2 className="text-lg font-medium">{t("products.title")}</h2>
        {products.isPending ? (
          <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
        ) : products.isError ? (
          <p role="alert" className="text-sm text-destructive">
            {t("common.error")}
          </p>
        ) : products.data.data.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("products.empty")}</p>
        ) : (
          <>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("products.name")}</TableHead>
                  <TableHead>{t("products.number")}</TableHead>
                  <TableHead>{t("products.reference")}</TableHead>
                  <TableHead className="text-right">
                    {t("products.price")}
                  </TableHead>
                  <TableHead>{t("products.status")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {products.data.data.map((p) => (
                  <TableRow key={p.id}>
                    <TableCell>{p.name}</TableCell>
                    <TableCell>{p.product_nr}</TableCell>
                    <TableCell>{p.reference}</TableCell>
                    {/* The API prints prices as exact strings; shown as-is, never parsed. */}
                    <TableCell className="text-right tabular-nums">
                      {p.price_incl}
                    </TableCell>
                    <TableCell>{p.status}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <p className="text-sm text-muted-foreground">
              {t("products.total", { count: products.data.meta.total })}
            </p>
          </>
        )}
      </section>
    </main>
  )
}
