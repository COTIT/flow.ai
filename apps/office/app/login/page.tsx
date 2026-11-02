"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useLogin, useSession } from "@genesy/api-client/auth"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { LoginForm } from "@genesy/ui/components/login-form"
import { Logo } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"
import { Notice } from "@/components/notice"

export default function LoginPage() {
  const t = useTranslations()
  const router = useRouter()
  const login = useLogin()
  const token = useSession((s) => s.token)
  const expired = useSession((s) => s.expired)

  React.useEffect(() => {
    if (token) router.replace("/products")
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
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 p-4">
      <Logo className="h-10" />
      <div className="gold-rule w-full max-w-sm" />
      {expired ? (
        <div className="w-full max-w-sm">
          <Notice tone="info">{t("session.expired")}</Notice>
        </div>
      ) : null}
      <LoginForm
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
      <div className="flex items-center gap-2">
        <LocaleSwitcher />
        <ThemeToggle label={t("common.theme")} />
      </div>
    </main>
  )
}
