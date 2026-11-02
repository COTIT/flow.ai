"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { useTheme } from "next-themes"
import { MoonIcon, SunIcon } from "lucide-react"

import { useLogin, useSession } from "@genesy/api-client/auth"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"
import { Logo } from "@genesy/ui/components/logo"
import { Notice } from "@genesy/ui/components/notice"
import { LocaleMenu } from "@/components/shell"

export default function LoginPage() {
  const t = useTranslations()
  const router = useRouter()
  const login = useLogin()
  const token = useSession((s) => s.token)
  const expired = useSession((s) => s.expired)
  const { resolvedTheme, setTheme } = useTheme()
  const errorId = React.useId()

  React.useEffect(() => {
    if (token) router.replace("/")
  }, [token, router])

  const status = login.error?.response?.status
  const error = !login.error
    ? null
    : status === 401
      ? t("auth.invalidLogin")
      : status === 429
        ? t("auth.tooManyAttempts")
        : status === 422
          ? t("auth.invalidInput")
          : t("common.error")

  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 p-6 max-md:p-4">
      <form
        className="glass-strong flex w-[min(440px,100%)] flex-col gap-5 rounded-[20px] p-8 max-md:p-6"
        aria-describedby={error ? errorId : undefined}
        onSubmit={(e) => {
          e.preventDefault()
          const form = new FormData(e.currentTarget)
          login.mutate({
            data: {
              email: String(form.get("email")),
              password: String(form.get("password")),
            },
          })
        }}
      >
        <Logo className="h-auto w-[168px]" />
        <div className="gold-rule w-12" />
        <div>
          <h1 className="text-[22px] leading-[1.3] font-medium">
            {t("auth.title")}
          </h1>
          <p className="text-[13px] text-muted-foreground">
            {t("login.subtitle")}
          </p>
        </div>
        {expired ? <Notice title={t("session.expired")} /> : null}
        {error ? (
          <div id={errorId}>
            <Notice variant="error" title={error} />
          </div>
        ) : null}
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="login-email">{t("auth.email")}</Label>
          <Input
            id="login-email"
            name="email"
            type="email"
            autoComplete="email"
            required
            aria-invalid={Boolean(error)}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="login-password">{t("auth.password")}</Label>
          <Input
            id="login-password"
            name="password"
            type="password"
            autoComplete="current-password"
            maxLength={255}
            required
            aria-invalid={Boolean(error)}
          />
        </div>
        <Button type="submit" size="lg" className="w-full" disabled={login.isPending}>
          {t("auth.submit")}
        </Button>
      </form>
      <div className="flex items-center gap-2">
        <LocaleMenu withName />
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setTheme(resolvedTheme === "dark" ? "light" : "dark")}
        >
          {/* Icons swap by CSS, so server and first client render match. */}
          <SunIcon aria-hidden="true" className="dark:hidden" />
          <MoonIcon aria-hidden="true" className="hidden dark:block" />
          {t("login.theme")}
        </Button>
      </div>
    </main>
  )
}
