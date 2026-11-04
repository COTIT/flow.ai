"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import { useTranslations } from "next-intl"
import { useTheme } from "next-themes"
import { MoonIcon, SunIcon } from "lucide-react"

import { useLogout, useSession } from "@genesy/api-client/auth"
import {
  useInvitesAnswer,
  useInvitesRegister,
  useInvitesStatus,
} from "@genesy/api-client/endpoints/invites"
import type { InviteStatusResponse } from "@genesy/api-client/model"
import { sessionStore } from "@genesy/api-client/session"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Logo } from "@genesy/ui/components/logo"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { Field } from "@/components/field"
import { LocaleMenu } from "@/components/shell"
import { LoadError } from "@/components/states"

/*
 * The invite code is a password stand-in: it lives only in this page's address. It is never
 * stored, logged or put in another URL, except the login link's `next` back to this page.
 */

/** The link from an invite email (Template Auth, as OfficeLogin): sign up, or accept / decline. */
export default function AcceptInvitationPage() {
  const t = useTranslations()
  const { resolvedTheme, setTheme } = useTheme()

  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 p-6 max-md:p-4">
      <div className="flex w-[min(440px,100%)] flex-col gap-5 rounded-[20px] glass-strong p-8 max-md:p-6">
        <Logo className="h-auto w-[168px]" />
        <div className="gold-rule w-12" />
        {/* useSearchParams needs a boundary for the static build; the skeleton is the card's shape. */}
        <React.Suspense fallback={<InviteSkeleton />}>
          <Invite />
        </React.Suspense>
      </div>
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

function Invite() {
  const t = useTranslations()
  const code = useSearchParams().get("code") ?? ""
  const hydrated = useSession((s) => s.hydrated)
  const token = useSession((s) => s.token)
  const status = useInvitesStatus(code, {
    query: {
      enabled: Boolean(code),
      // A refused code stays refused; only a lost connection is worth another try.
      retry: (count, error) => !error.response && count < 3,
    },
  })

  if (!code)
    return (
      <>
        <Head title={t("invite.title")} />
        <Notice variant="error" title={t("invite.incomplete")} />
      </>
    )
  if (status.isPending || !hydrated) return <InviteSkeleton />
  if (status.isError)
    return (
      <>
        <Head title={t("invite.title")} />
        {status.error.response ? (
          <Notice variant="error" title={t("invite.invalid")} />
        ) : (
          <LoadError
            error={status.error}
            onRetry={() => void status.refetch()}
          />
        )}
      </>
    )

  const invite = status.data.data
  if (invite.status !== "pending")
    return (
      <>
        <Head title={t("invite.title")} />
        <Notice
          variant={invite.status === "expired" ? "warning" : "info"}
          title={
            t.has(`invite.ended.${invite.status}`)
              ? t(`invite.ended.${invite.status}`)
              : t("invite.invalid")
          }
          action={
            <Link
              href="/login"
              className="text-sm font-medium underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"
            >
              {t("invite.goLogin")}
            </Link>
          }
        />
      </>
    )
  if (invite.new_user) return <Register invite={invite} code={code} />
  if (token) return <Answer invite={invite} code={code} />
  return (
    <>
      <Head
        title={t("invite.join", { workspace: invite.display_name })}
        subtitle={invite.email}
      />
      <Notice title={t("invite.loginFirst")} />
      <Link
        href={`/login?next=${encodeURIComponent(`/accept-invitation?code=${code}`)}`}
        className={buttonVariants({ size: "lg", className: "w-full" })}
      >
        {t("auth.submit")}
      </Link>
    </>
  )
}

function Head({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <div>
      <h1 className="text-[22px] leading-[1.3] font-medium break-words">
        {title}
      </h1>
      {subtitle ? (
        <p className="text-[13px] break-all text-muted-foreground">
          {subtitle}
        </p>
      ) : null}
    </div>
  )
}

function InviteSkeleton() {
  return (
    <div aria-busy="true" className="flex flex-col gap-5">
      <div className="flex flex-col gap-2">
        <Skeleton className="h-7 w-56" />
        <Skeleton className="h-4 w-40" />
      </div>
      <Skeleton className="h-11 w-full rounded-xl" />
      <Skeleton className="h-11 w-full rounded-xl" />
      <Skeleton className="h-10 w-full rounded-xl" />
    </div>
  )
}

/** New to Genesy: make a login, which joins the account at once. */
function Register({
  invite,
  code,
}: {
  invite: InviteStatusResponse
  code: string
}) {
  const t = useTranslations()
  const router = useRouter()
  const register = useInvitesRegister({
    mutation: {
      onSuccess: ({ data }) => {
        sessionStore.getState().signIn(data)
        router.replace("/")
      },
    },
  })
  const errors = fieldErrors(register.error)

  return (
    <form
      className="flex flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault()
        const f = new FormData(e.currentTarget)
        const text = (k: string) => String(f.get(k) ?? "")
        register.mutate({
          data: {
            email: invite.email,
            first_name: text("first_name").trim(),
            last_name: text("last_name").trim(),
            password: text("password"),
            invite_code: code,
          },
        })
      }}
    >
      <Head
        title={t("invite.join", { workspace: invite.display_name })}
        subtitle={t("invite.signUp")}
      />
      <Field id="inv-email" label={t("auth.email")} errors={errors.email}>
        <Input value={invite.email} readOnly autoComplete="email" />
      </Field>
      <div className="grid gap-5 sm:grid-cols-2">
        <Field
          id="inv-first"
          label={t("invite.firstName")}
          errors={errors.first_name}
        >
          <Input
            name="first_name"
            autoComplete="given-name"
            maxLength={55}
            required
          />
        </Field>
        <Field
          id="inv-last"
          label={t("invite.lastName")}
          errors={errors.last_name}
        >
          <Input
            name="last_name"
            autoComplete="family-name"
            maxLength={55}
            required
          />
        </Field>
      </div>
      <div className="flex flex-col gap-1.5">
        <Field
          id="inv-password"
          label={t("auth.password")}
          errors={errors.password}
        >
          <Input
            name="password"
            type="password"
            autoComplete="new-password"
            maxLength={255}
            required
          />
        </Field>
        <p className="text-xs text-muted-foreground">
          {t("invite.passwordRule")}
        </p>
      </div>
      <ApiError error={register.error} fallback={t("common.error")} />
      <Button
        type="submit"
        size="lg"
        className="w-full"
        disabled={register.isPending}
      >
        {t("invite.create")}
      </Button>
    </form>
  )
}

/** Already has a login and is signed in: join the account, or say no. */
function Answer({
  invite,
  code,
}: {
  invite: InviteStatusResponse
  code: string
}) {
  const t = useTranslations()
  const router = useRouter()
  const answer = useInvitesAnswer()
  // The saved login lists only the accounts it was given, so a new login brings the new one.
  const logout = useLogout()
  const declined = answer.isSuccess && !answer.variables.data.accept
  const joined = answer.isSuccess && answer.variables.data.accept
  const reply = (accept: boolean) =>
    answer.mutate({ data: { accept, invite_code: code } })

  return (
    <>
      <Head
        title={t("invite.join", { workspace: invite.display_name })}
        subtitle={invite.email}
      />
      {declined ? (
        <Notice title={t("invite.declined")} />
      ) : joined ? (
        <>
          <Notice
            variant="success"
            title={t("invite.joined", { workspace: invite.display_name })}
          >
            {t("invite.joinedHint")}
          </Notice>
          <Button
            size="lg"
            className="w-full"
            disabled={logout.isPending}
            // Signing out swaps this view away, so go on from the promise, not a mutate callback.
            onClick={() =>
              void logout
                .mutateAsync()
                .finally(() => router.replace("/login"))
                .catch(() => undefined)
            }
          >
            {t("invite.loginAgain")}
          </Button>
        </>
      ) : (
        <>
          <ApiError error={answer.error} fallback={t("common.error")} />
          <div className="flex flex-col gap-3">
            <Button
              size="lg"
              className="w-full"
              disabled={answer.isPending}
              onClick={() => reply(true)}
            >
              {t("invite.accept")}
            </Button>
            <Button
              variant="outline"
              size="lg"
              className="w-full"
              disabled={answer.isPending}
              onClick={() => reply(false)}
            >
              {t("invite.decline")}
            </Button>
          </div>
        </>
      )}
    </>
  )
}
