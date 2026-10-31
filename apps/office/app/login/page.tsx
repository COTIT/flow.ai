"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useLogin, useSession } from "@genesy/api-client/auth"
import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { LoginForm } from "@genesy/ui/components/login-form"

export default function LoginPage() {
  const t = useTranslations()
  const router = useRouter()
  const login = useLogin()
  const token = useSession((s) => s.token)

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
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 p-4">
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
      <LocaleSwitcher />
    </main>
  )
}
