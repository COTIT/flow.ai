"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import {
  useTillLogin,
  useTillLoginUsers,
} from "@genesy/api-client/endpoints/till"
import type { TillUserBrief } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { AuthTop } from "@/components/auth-top"
import { Keypad, type PadKey } from "@/components/keypad"
import { deviceAuth, tillStore, useTill } from "@/lib/till-store"

/** "AM" for Amina, "JM" for John Mwita: the tile's avatar. */
function initials(name: string) {
  const words = name.trim().split(/\s+/)
  return (
    words.length > 1 ? words[0]![0]! + words.at(-1)![0]! : name.slice(0, 2)
  ).toUpperCase()
}

/** shortcut: the old input's limit; the API judges the PIN (4 to 8 digits). */
const MAX_PIN = 8

/** Till user tiles and a PIN pad; the PIN goes with the device key. */
export default function LoginPage() {
  const t = useTranslations()
  const router = useRouter()
  const hydrated = useTill((s) => s.hydrated)
  const device = useTill((s) => s.device)
  const session = useTill((s) => s.session)
  const ended = useTill((s) => s.ended)
  const [user, setUser] = React.useState<TillUserBrief | null>(null)
  const [pin, setPin] = React.useState("")

  const users = useTillLoginUsers(device?.workspaceId ?? "", {
    query: { enabled: Boolean(device), retry: false },
    request: deviceAuth(device),
  })
  const login = useTillLogin({
    request: deviceAuth(device),
    mutation: {
      onSuccess: ({ data }) => tillStore.getState().signIn(data),
      onError: () => setPin(""),
    },
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

  const pick = (u: TillUserBrief | null) => {
    login.reset()
    setPin("")
    setUser(u)
  }
  const press = (k: PadKey) =>
    setPin((p) =>
      k === "back"
        ? p.slice(0, -1)
        : k === "clear"
          ? ""
          : (p + k).slice(0, MAX_PIN)
    )
  const submit = () => {
    if (!user || !pin || login.isPending) return
    login.mutate({
      workspace: device.workspaceId,
      data: { till_user_id: user.id, pin },
    })
  }
  const dots = Math.max(4, pin.length)

  return (
    <div className="till">
      <AuthTop />
      <main className="split">
        <section className="till-card gap-3.5" aria-labelledby="login-title">
          <div>
            <h1 id="login-title" className="till-h3">
              {t("login.title")}
            </h1>
            <p className="text-[13px] text-muted-foreground">
              {t("login.tap")}
            </p>
          </div>
          {/* Room for the empty state, so the PIN card below (portrait) never jumps. */}
          <div className="flex min-h-[174px] flex-col gap-3.5">
            {keyRefused ? (
              <>
                <Notice variant="error" title={t("login.keyRefused")} />
                <Button
                  className="btn-64 w-full"
                  onClick={() => {
                    tillStore.getState().unpair()
                    router.replace("/setup")
                  }}
                >
                  {t("login.setupAgain")}
                </Button>
              </>
            ) : users.isError ? (
              <Notice variant="error" title={t("login.usersFailed")} />
            ) : users.isPending ? (
              <div
                className="users"
                role="status"
                aria-label={t("common.loading")}
              >
                {[0, 1, 2].map((i) => (
                  <div key={i} className="utile">
                    <Skeleton className="size-[52px] rounded-full" />
                    <Skeleton className="h-[22px] w-16" />
                  </div>
                ))}
              </div>
            ) : !users.data.data.length ? (
              <EmptyState title={t("login.noUsers")} />
            ) : (
              <ul aria-label={t("login.who")} className="users">
                {users.data.data.map((u) => (
                  <li key={u.id}>
                    <button
                      type="button"
                      className="utile"
                      aria-pressed={user?.id === u.id}
                      onClick={() => pick(u)}
                    >
                      <span className="avatar" aria-hidden>
                        {initials(u.name)}
                      </span>
                      <span>{u.name}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
          <p className="mt-auto text-xs text-muted-foreground">
            {t("login.missing")}
          </p>
        </section>

        <form
          className="till-card strong items-stretch gap-4"
          aria-label={t("login.enterPin")}
          onSubmit={(e) => {
            e.preventDefault()
            submit()
          }}
        >
          <div className="flex items-center gap-3">
            <span className="avatar md" aria-hidden>
              {user ? initials(user.name) : null}
            </span>
            <div className="min-w-0 grow">
              <p className="text-lg font-semibold">
                {user ? user.name : t("login.enterPin")}
              </p>
              <p className="text-[13px] text-muted-foreground">
                {user ? t("login.enterPin") : t("login.pickFirst")}
              </p>
            </div>
            {user ? (
              <Button
                type="button"
                variant="outline"
                className="btn-44"
                onClick={() => pick(null)}
              >
                {t("login.notYou")}
              </Button>
            ) : null}
          </div>
          <div
            className="dots"
            role="img"
            aria-label={t("login.dots", { count: pin.length, total: dots })}
          >
            {Array.from({ length: dots }, (_, i) => (
              <i key={i} className={i < pin.length ? "on" : undefined} />
            ))}
          </div>
          {ended === "expired" ? (
            <Notice variant="info" title={t("login.expired")} />
          ) : null}
          {error ? (
            <Notice variant="error" className="px-3.5 py-2.5" title={error} />
          ) : null}
          <Keypad
            fn="clear"
            keyboard
            disabled={!user}
            onKey={press}
            onEnter={submit}
          />
          <Button
            type="submit"
            className="btn-64 w-full"
            disabled={!user || !pin || login.isPending}
          >
            {t("login.submit")}
          </Button>
        </form>
      </main>
    </div>
  )
}
