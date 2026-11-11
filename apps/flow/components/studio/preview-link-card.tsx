"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import {
  useStorefrontDesignPreviewLinkCreate,
  useStorefrontDesignPreviewLinkRevoke,
  useStorefrontDesignPreviewLinks,
} from "@genesy/api-client/endpoints/storefront-design"
import { useIdempotencyKey } from "@genesy/api-client/idempotency"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { formatDateTime } from "@genesy/ui/lib/format"
import { shopPreviewHref } from "@/lib/links"
import {
  linkView,
  rememberToken,
  studioStore,
  tokenFor,
  useStudio,
} from "@/lib/studio"

/** Time left until an ISO instant (negative once passed). */
const msUntil = (iso: string) => new Date(iso).getTime() - Date.now()

const TITLE = {
  none: "readyTitle",
  active: "readyTitle",
  hidden: "readyTitle",
  expired: "expiredTitle",
  revoked: "revokedTitle",
  ended: "endedTitle",
} as const
const ACCESS = {
  none: "accessNone",
  active: "accessActive",
  hidden: "accessHidden",
  expired: "accessExpired",
  revoked: "accessRevoked",
  ended: "accessEnded",
} as const

/**
 * The draft's expiring full-page preview: status and expiry from the API, the token only from this
 * tab's memory. Opening is a plain new-tab link to the Shop, never while a review is open here
 * (a hidden tab cancels open reviews).
 */
export function PreviewLinkCard({
  workspaceId,
  draftId,
  draftLabel,
  liveLabel,
  shop,
  blocked,
  onAnother,
  onReview,
}: {
  workspaceId: string
  draftId: string
  draftLabel: string
  liveLabel: string
  shop: string | null
  /** A publish card or a voice review is open: no new tab now. */
  blocked: boolean
  onAnother: () => void
  onReview: () => void
}) {
  const t = useTranslations("studio")
  const tAll = useTranslations()
  const locale = useLocale()
  const links = useStorefrontDesignPreviewLinks(
    workspaceId,
    { revision_id: draftId },
    { query: { refetchOnWindowFocus: true } }
  )
  const token = useStudio((s) =>
    s.token?.revisionId === draftId ? s.token.token : null
  )
  const awaiting = useStudio((s) => s.awaiting)
  const key = useIdempotencyKey()
  const create = useStorefrontDesignPreviewLinkCreate({
    request: key.request,
    mutation: {
      onSuccess: (answer) => {
        if (answer.data.token) rememberToken(draftId, answer.data.token)
        return links.refetch()
      },
      onSettled: (_data, error) => {
        key.settle(error)
        // The answer held the token: drop it from the mutation cache at once.
        if (!error) create.reset()
      },
    },
  })
  const revoke = useStorefrontDesignPreviewLinkRevoke({
    mutation: { onSuccess: () => links.refetch() },
  })
  const view = linkView(links.data?.data ?? [], token)
  const issue = React.useCallback(() => {
    if (!shop || create.isPending) return
    create.mutate({ workspace: workspaceId, data: { revision_id: draftId } })
  }, [shop, create, workspaceId, draftId])

  // A draft born from this tab's request gets its link at once; any other draft waits for a click.
  React.useEffect(() => {
    if (!awaiting || awaiting.from === draftId || !links.isSuccess) return
    studioStore.setState({ awaiting: null })
    if (!tokenFor(draftId)) issue()
  }, [awaiting, draftId, links.isSuccess, issue])

  // Expiry is the server's: ask again when the newest link should end.
  const expires = view.link?.status === "active" ? view.link.expires_at : null
  React.useEffect(() => {
    if (!expires) return
    const wait = msUntil(expires)
    if (wait > 2 ** 31 - 1) return
    const timer = setTimeout(() => void links.refetch(), Math.max(0, wait) + 1000)
    return () => clearTimeout(timer)
  }, [expires, links])

  const href = token ? shopPreviewHref(token, shop) : ""
  const busy = create.isPending || revoke.isPending

  if (links.isPending)
    return <Skeleton className="h-40 w-full" aria-label={tAll("common.loading")} />
  return (
    <section
      aria-label={t("region")}
      className="flex flex-col gap-3 rounded-2xl border bg-card p-4"
    >
      <p className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">
        {t(TITLE[view.state])}
      </p>
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm">
        <dt className="text-muted-foreground">{t("revision")}</dt>
        <dd>{t("revisionValue", { draft: draftLabel, live: liveLabel })}</dd>
        <dt className="text-muted-foreground">{t("status")}</dt>
        <dd>{t("unpublished")}</dd>
        <dt className="text-muted-foreground">{t("access")}</dt>
        <dd>{t(ACCESS[view.state])}</dd>
        {view.link ? (
          <>
            <dt className="text-muted-foreground">{t("expires")}</dt>
            <dd>{formatDateTime(view.link.expires_at, locale)}</dd>
          </>
        ) : null}
      </dl>
      {view.state === "hidden" ? (
        <p className="text-sm text-muted-foreground">{t("hiddenNote")}</p>
      ) : null}
      {!shop ? <Notice variant="info" title={t("noShop")} /> : null}
      {links.isError ? (
        <ApiError error={links.error} fallback={tAll("common.error")} />
      ) : null}
      <ApiError error={create.error ?? revoke.error} fallback={tAll("common.error")} />
      {blocked && view.state === "active" ? (
        <p className="text-sm text-muted-foreground">{t("openBlocked")}</p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        {view.state === "active" && href && !blocked ? (
          <a
            href={href}
            target="_blank"
            rel="noopener noreferrer"
            referrerPolicy="no-referrer"
            aria-label={t("openLabel")}
            className={buttonVariants({ className: "min-h-11" })}
          >
            {t("open")}
          </a>
        ) : null}
        {view.state !== "active" ? (
          <Button className="min-h-11" disabled={!shop || busy} onClick={issue}>
            {t(view.state === "none" ? "generate" : "regenerate")}
          </Button>
        ) : null}
        <Button variant="outline" className="min-h-11" onClick={onAnother}>
          {t("another")}
        </Button>
        <Button
          variant="outline"
          className="min-h-11"
          disabled={blocked}
          onClick={onReview}
        >
          {t("review")}
        </Button>
        {(view.state === "active" || view.state === "hidden") && view.link ? (
          <Button
            variant="outline"
            className="min-h-11"
            disabled={busy}
            onClick={() =>
              revoke.mutate({ workspace: workspaceId, link: view.link!.link_id })
            }
          >
            {t("revoke")}
          </Button>
        ) : null}
      </div>
    </section>
  )
}
