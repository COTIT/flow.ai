"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import {
  useTillLogin,
  useTillLoginUsers,
} from "@genesy/api-client/endpoints/till"
import type { TillUserBrief } from "@genesy/api-client/model"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"
import { Logo } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { Empty, Loading, Notice } from "@/components/state"
import { deviceAuth, tillStore, useTill } from "@/lib/till-store"

/** Till user picker and PIN, sent with the device key. */
export default function LoginPage() {
  const t = useTranslations()
  const router = useRouter()
  const hydrated = useTill((s) => s.hydrated)
  const device = useTill((s) => s.device)
  const session = useTill((s) => s.session)
  const ended = useTill((s) => s.ended)
  const [user, setUser] = React.useState<TillUserBrief | null>(null)

  const users = useTillLoginUsers(device?.workspaceId ?? "", {
    query: { enabled: Boolean(device), retry: false },
    request: deviceAuth(device),
  })
  const login = useTillLogin({
    request: deviceAuth(device),
    mutation: { onSuccess: ({ data }) => tillStore.getState().signIn(data) },
  })

  React.useEffect(() => {
    if (!hydrated) return
    if (!device) router.replace("/setup")
    else if (session) router.replace("/sale")
  }, [hydrated, device, session, router])

  if (!hydrated || !device) return null

  const keyRefused = [401, 403, 404].includes(
    users.error?.response?.status ?? 0
  )
  const status = login.error?.response?.status
  const error = !login.error
    ? null
    : status === 401
      ? t("login.wrongPin")
      : status === 429
        ? t("login.locked")
        : status === 422
          ? t("login.pinFormat")
          : t("common.error")

  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 p-4">
      <Logo className="h-10" />
      <div className="flex w-full max-w-sm flex-col gap-4">
        <h1 className="text-xl font-medium">{t("login.title")}</h1>
        {ended === "expired" ? (
          <Notice kind="info">{t("login.expired")}</Notice>
        ) : null}
        {keyRefused ? (
          <>
            <Notice kind="error">{t("login.keyRefused")}</Notice>
            <Button
              onClick={() => {
                tillStore.getState().unpair()
                router.replace("/setup")
              }}
            >
              {t("login.setupAgain")}
            </Button>
          </>
        ) : users.isError ? (
          <Notice kind="error">{t("login.usersFailed")}</Notice>
        ) : users.isPending ? (
          <Loading>{t("common.loading")}</Loading>
        ) : !users.data.data.length ? (
          <Empty>{t("login.noUsers")}</Empty>
        ) : !user ? (
          <ul aria-label={t("login.who")} className="grid grid-cols-2 gap-2">
            {users.data.data.map((u) => (
              <li key={u.id}>
                <Button
                  variant="outline"
                  className="h-14 w-full"
                  onClick={() => setUser(u)}
                >
                  {u.name}
                </Button>
              </li>
            ))}
          </ul>
        ) : (
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault()
              const pin = String(new FormData(e.currentTarget).get("pin"))
              login.mutate({
                workspace: device.workspaceId,
                data: { till_user_id: user.id, pin },
              })
            }}
          >
            <p className="font-medium">{user.name}</p>
            <div className="flex flex-col gap-2">
              <Label htmlFor="login-pin">{t("login.pin")}</Label>
              <Input
                id="login-pin"
                name="pin"
                type="password"
                inputMode="numeric"
                autoComplete="off"
                maxLength={8}
                required
                autoFocus
                aria-invalid={Boolean(error)}
              />
            </div>
            {error ? <Notice kind="error">{error}</Notice> : null}
            <div className="flex gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  login.reset()
                  setUser(null)
                }}
              >
                {t("login.back")}
              </Button>
              <Button
                type="submit"
                className="flex-1"
                disabled={login.isPending}
              >
                {t("login.submit")}
              </Button>
            </div>
          </form>
        )}
      </div>
      <div className="flex items-center gap-2">
        <LocaleSwitcher />
        <ThemeToggle label={t("common.theme")} />
      </div>
    </main>
  )
}
