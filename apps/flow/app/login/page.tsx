"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useLogin, useSession } from "@genesy/api-client/auth"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { LoginForm } from "@genesy/ui/components/login-form"
import { Logo } from "@genesy/ui/components/logo"
import { Notice } from "@genesy/ui/components/notice"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { safeNext } from "@/lib/next-path"

export default function LoginPage() {
  const t = useTranslations()
  const router = useRouter()
  const login = useLogin()
  const token = useSession((s) => s.token)
  const expired = useSession((s) => s.expired)

  React.useEffect(() => {
    if (token)
      router.replace(
        safeNext(new URLSearchParams(window.location.search).get("next"), window.location.origin)
      )
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
          : t("flow.error")

  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 p-6 max-md:p-4">
      <div
        className="flex w-[min(440px,100%)] flex-col gap-5 rounded-[20px] glass-strong p-8 max-md:p-6"
        aria-busy={login.isPending}
      >
        <Logo className="h-auto w-[168px]" />
        <div className="gold-rule w-12" />
        <p className="text-[13px] text-muted-foreground">
          {t("flow.loginSubtitle")}
        </p>
        {expired ? <Notice title={t("flow.sessionEnded")} /> : null}
        <LoginForm
          className="max-w-none"
          labels={{
            title: t("auth.title"),
            email: t("auth.email"),
            password: t("auth.password"),
            submit: t("auth.submit"),
          }}
          pending={login.isPending}
          error={error}
          onSubmit={(data) => login.mutate({ data })}
        />
      </div>
      <div className="flex items-center gap-2">
        <LocaleSwitcher />
        <ThemeToggle label={t("common.theme")} />
      </div>
    </main>
  )
}
