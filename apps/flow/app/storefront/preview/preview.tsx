"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"
import { useRouter } from "next/navigation"

import { useSession } from "@genesy/api-client/auth"
import { useStorefrontDesignPreview } from "@genesy/api-client/endpoints/storefront-design"
import { Notice } from "@genesy/ui/components/notice"
import { StorefrontPage } from "@genesy/ui/components/storefront-page"
import { canRenderStorefrontPage } from "@genesy/ui/lib/storefront-design"
import { followOtherTabs } from "@/lib/other-tabs"
import {
  PROPOSAL_READY,
  proposalFromParent,
  type Proposal,
} from "@/lib/storefront-proposal"

const noChange = () => () => {}

/** null on the server; in the browser, whether a page (Flow's own, by CSP) frames this one. */
const useFramed = () =>
  React.useSyncExternalStore(
    noChange,
    () => window.top !== window,
    () => null
  )

/** Opened on its own: back to the review page. Framed: inert, nothing clicks or submits. */
function useInertFrame() {
  const router = useRouter()
  React.useEffect(() => {
    // Opened on its own (a bookmark, a pasted link): back to the review page.
    if (window.top === window) router.replace("/storefront")
    // Defence in depth: the renderer's preview mode has no links or forms anyway.
    const stop = (event: Event) => event.preventDefault()
    document.addEventListener("click", stop, true)
    document.addEventListener("submit", stop, true)
    return () => {
      document.removeEventListener("click", stop, true)
      document.removeEventListener("submit", stop, true)
    }
  }, [router])
}

function Page({ data }: { data: Proposal }) {
  const t = useTranslations("flow.storefront")
  const locale = useLocale()
  return (
    <main className="min-w-0 p-4 md:p-6">
      <StorefrontPage
        document={data.document}
        collections={data.collections}
        locale={locale}
        preview
        headings="page"
        labels={{
          inStock: t("inStock"),
          low: t("lowStock"),
          soldOut: t("soldOut"),
          imageSoon: t("imageSoon"),
          emptyProducts: t("emptyProducts"),
          shopCta: t("shopCta"),
        }}
      />
    </main>
  )
}

/**
 * A proposed design (an edit's dry run) handed over by the review page around this frame. It
 * fetches nothing: it says ready to its parent on this origin and takes the first renderable
 * proposal from exactly that window and origin; everything else is ignored.
 */
export function ProposalPreview() {
  const t = useTranslations("flow.storefront")
  const framed = useFramed()
  const [proposal, setProposal] = React.useState<Proposal | "invalid" | null>(null)
  useInertFrame()
  React.useEffect(() => {
    if (window.top === window) return
    const onMessage = (event: MessageEvent) => {
      const got = proposalFromParent(event, window.parent, location.origin, canRenderStorefrontPage)
      if (got) setProposal((first) => first ?? got)
    }
    window.addEventListener("message", onMessage)
    window.parent.postMessage({ type: PROPOSAL_READY }, location.origin)
    return () => window.removeEventListener("message", onMessage)
  }, [])
  if (!framed) return null
  if (proposal === "invalid")
    return (
      <main className="p-4">
        <Notice variant="error" title={t("unsupported")} />
      </main>
    )
  return proposal ? <Page data={proposal} /> : null
}

/**
 * One revision through the shared renderer, in preview mode (no links). The login and workspace
 * come from this origin's saved session, like any Flow page; the token goes in the
 * Authorization header, never the URL. The API refuses members (403) and foreign ids (404).
 */
export function Preview({ revisionId }: { revisionId: string | null }) {
  const t = useTranslations("flow.storefront")
  const hydrated = useSession((s) => s.hydrated)
  const token = useSession((s) => s.token)
  const workspaceId = useSession((s) => s.workspaceId)
  // The login and workspace this frame was opened for; after a change in any other tab or the
  // page around it, it shows and asks nothing until the review page opens a new frame.
  const identity = hydrated && token ? `${workspaceId}:${token}` : null
  const [opened, setOpened] = React.useState<string | null>(null)
  if (identity && opened === null) setOpened(identity)
  const stale = opened !== null && identity !== opened
  React.useEffect(() => followOtherTabs(), [])
  const framed = useFramed()
  useInertFrame()

  const ready = Boolean(
    framed && hydrated && token && workspaceId && revisionId && !stale
  )
  const preview = useStorefrontDesignPreview(
    workspaceId ?? "",
    { revision_id: revisionId ?? "" },
    { query: { enabled: ready, retry: false } }
  )

  if (!framed || !hydrated) return null
  if (!token)
    return (
      <main className="p-4">
        <Notice variant="info" title={t("loggedOut")} />
      </main>
    )
  if (stale) return null
  if (!revisionId || !workspaceId || preview.isError)
    return (
      <main className="p-4">
        <Notice variant="error" title={t("unavailable")} />
      </main>
    )
  const data = preview.data?.data
  if (data && !canRenderStorefrontPage(data.document, data.collections))
    return (
      <main className="p-4">
        <Notice variant="error" title={t("unsupported")} />
      </main>
    )
  return data ? <Page data={data} /> : <main className="min-w-0 p-4 md:p-6" />
}
