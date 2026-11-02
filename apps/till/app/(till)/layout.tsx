"use client"

import * as React from "react"
import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { tillMe } from "@genesy/api-client/endpoints/till"
import { Notice } from "@genesy/ui/components/notice"
import { TillTop } from "@/components/till-top"
import { tillStore, useTill } from "@/lib/till-store"
import { useOnline } from "@/lib/use-online"

/** Logged-in frame for the till screens: guard, top bar, offline warning. */
export default function TillLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const t = useTranslations()
  const router = useRouter()
  const pathname = usePathname()
  const hydrated = useTill((s) => s.hydrated)
  const device = useTill((s) => s.device)
  const session = useTill((s) => s.session)
  const online = useOnline()
  React.useEffect(() => {
    if (!hydrated) return
    if (!device) router.replace("/setup")
    else if (!session) router.replace("/login")
  }, [hydrated, device, session, router])

  // The stored login may predate a field (currency, accepted cash) or a settings change:
  // refresh the register from /till/me on start and whenever the till comes back online.
  const token = session?.token
  React.useEffect(() => {
    if (!token || !device) return
    const refresh = () =>
      tillMe(device.workspaceId)
        .then(({ data }) => {
          const now = tillStore.getState().session
          if (now?.token === token)
            tillStore.getState().signIn({ ...now, ...data })
        })
        .catch(() => undefined) // offline or ended: the stored copy stands, a 401 logs out
    void refresh()
    window.addEventListener("online", refresh)
    return () => window.removeEventListener("online", refresh)
  }, [token, device])

  if (!hydrated || !device || !session)
    return <main className="till" aria-busy aria-label={t("common.loading")} />

  return (
    <div className="till">
      <TillTop />
      {/* The queue shows its own warning, with Sync now. */}
      {online || pathname.startsWith("/queue") ? null : (
        <Notice
          variant="warning"
          className="mx-3 mt-3 print:hidden"
          title={t("offline.title")}
        >
          {t("offline.body")}
        </Notice>
      )}
      {children}
    </div>
  )
}
