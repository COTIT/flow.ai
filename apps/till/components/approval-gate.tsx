"use client"

import * as React from "react"
import { ShieldCheck } from "lucide-react"
import { useTranslations } from "next-intl"

import { authLogin, authLogout } from "@verzo/api-client/endpoints/auth"
import { tillPrivilegedSession } from "@verzo/api-client/endpoints/till"
import { Button } from "@verzo/ui/components/button"
import { Input } from "@verzo/ui/components/input"
import { Label } from "@verzo/ui/components/label"
import { Notice } from "@verzo/ui/components/notice"
import {
  approvalStore,
  holds,
  privileged,
  useApproval,
  type Approval,
} from "@/lib/approval"
import { useTill } from "@/lib/till-store"
import { useOnline } from "@/lib/use-online"

const status = (e: unknown) =>
  (e as { response?: { status?: number } } | null)?.response?.status

/** Ends the approval here and the member's login on the server (best effort when offline). */
export function endApproval(a: Approval | null) {
  approvalStore.getState().end()
  if (a)
    void authLogout({ headers: { Authorization: `Bearer ${a.token}` } }).catch(
      () => undefined
    )
}

/**
 * A member approves a refund, return or drawer adjustment with their own email and password
 * (never a till PIN). The login lives in memory only. Offline: no form, the action needs a
 * live approval. `right` is the permission the screen needs (`cashregister.refund` / `.adjust`).
 */
export function ApprovalGate({
  right,
  children,
}: {
  right: "cashregister.refund" | "cashregister.adjust"
  children: React.ReactNode
}) {
  const t = useTranslations("approval")
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  const cashier = useTill((s) => s.session?.user.name)
  const approval = useApproval()
  const online = useOnline()
  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [error, setError] = React.useState<string | null>(null)
  const [pending, setPending] = React.useState(false)

  if (!online)
    return (
      <Notice variant="warning" title={t("offlineTitle")}>
        {t("offlineBody")}
      </Notice>
    )

  if (approval && holds(approval, right))
    return (
      <>
        <div className="till-card tight flex-row flex-wrap items-center justify-between">
          <span className="flex min-w-0 items-center gap-2">
            <ShieldCheck aria-hidden className="size-5 shrink-0" />
            <span role="status" className="min-w-0 break-words">
              {t("approvedBy", {
                email: approval.email,
                cashier: cashier ?? "",
              })}
            </span>
          </span>
          <Button
            variant="outline"
            className="btn-44"
            onClick={() => endApproval(approval)}
          >
            {t("end")}
          </Button>
        </div>
        {children}
      </>
    )

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (pending) return
    setError(null)
    setPending(true)
    let token: string | null = null
    try {
      const { data } = await authLogin({ email: email.trim(), password })
      token = data.access_token
      const { data: session } = await tillPrivilegedSession(
        workspace,
        privileged({ token })
      )
      if (!session.member.permissions.includes(right)) {
        setError(t("missingRight"))
        void authLogout({
          headers: { Authorization: `Bearer ${token}` },
        }).catch(() => undefined)
        return
      }
      approvalStore.getState().approve({ token, email: email.trim(), session })
      setPassword("")
    } catch (err) {
      const s = status(err)
      setError(
        !token
          ? s === 401
            ? t("wrongLogin")
            : s === 429
              ? t("tooMany")
              : s === 403
                ? t("noAccount")
                : t("failed")
          : s === 403
            ? t("missingRight")
            : s === 404
              ? t("otherAccount")
              : s === 401
                ? t("tillEnded")
                : t("failed")
      )
      if (token)
        void authLogout({
          headers: { Authorization: `Bearer ${token}` },
        }).catch(() => undefined)
    } finally {
      setPending(false)
    }
  }

  return (
    <form
      onSubmit={submit}
      aria-labelledby="approval-title"
      className="till-card strong w-[min(520px,100%)] self-center"
    >
      <div className="flex items-center gap-3">
        <span className="chip-ic gold">
          <ShieldCheck aria-hidden className="size-5" />
        </span>
        <div className="min-w-0">
          <h1 id="approval-title" className="till-h2">
            {t("title")}
          </h1>
          <p className="text-sm text-muted-foreground">{t("hint")}</p>
        </div>
      </div>
      {approval ? <Notice variant="error" title={t("missingRight")} /> : null}
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="approval-email">{t("email")}</Label>
        <Input
          id="approval-email"
          type="email"
          autoComplete="off"
          required
          className="h-12"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
      </div>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="approval-password">{t("password")}</Label>
        <Input
          id="approval-password"
          type="password"
          autoComplete="off"
          required
          className="h-12"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
      </div>
      {error ? <Notice variant="error" title={error} /> : null}
      <button type="submit" className="charge" disabled={pending}>
        <span>{t("submit")}</span>
        <ShieldCheck aria-hidden className="size-[22px]" />
      </button>
    </form>
  )
}
