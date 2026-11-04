"use client"

import * as React from "react"

import Link from "next/link"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"

/**
 * A list or page that failed to load: the API's words (or a plain fallback) and a retry.
 * On a detail page (`backHref`), a 404 is "not found" with a way back, not a retry.
 */
export function LoadError({
  error,
  onRetry,
  backHref,
}: {
  error: unknown
  onRetry: () => void
  backHref?: string
}) {
  const t = useTranslations()
  const response = (
    error as {
      response?: { status?: number; data?: { message?: string } }
    } | null
  )?.response
  if (backHref && response?.status === 404)
    return (
      <EmptyState
        title={t("states.notFound")}
        description={t("states.notFoundText")}
        action={
          <Link
            className="text-sm font-medium underline-offset-4 hover:underline"
            href={backHref}
          >
            {t("states.back")}
          </Link>
        }
      />
    )
  return (
    <Notice
      variant="error"
      title={response?.data?.message ?? t("common.error")}
      action={
        <Button variant="outline" size="sm" onClick={onRetry}>
          {t("states.retry")}
        </Button>
      }
    />
  )
}

/** A page the member's rights do not cover, opened by its address: said plainly, with a way home. */
export function NoAccess() {
  const t = useTranslations("states")
  return (
    <EmptyState
      title={t("noAccess")}
      description={t("noAccessText")}
      action={
        <Link
          className="text-sm font-medium underline-offset-4 hover:underline"
          href="/"
        >
          {t("goHome")}
        </Link>
      }
    />
  )
}

/** A picker that only holds the first page of a long list says so. */
export function Capped({
  meta,
}: {
  meta?: { total: number; per_page: number }
}) {
  const t = useTranslations()
  if (!meta || meta.total <= meta.per_page) return null
  return (
    <Notice
      variant="warning"
      title={t("states.capped", { shown: meta.per_page, total: meta.total })}
    />
  )
}

/** Nothing to list. With `onClear`, the filters found nothing, not the data. */
export function Empty({
  title,
  onClear,
}: {
  title: string
  onClear?: () => void
}) {
  const t = useTranslations()
  return onClear ? (
    <EmptyState
      title={t("states.noMatches")}
      description={t("states.noMatchesText")}
      action={
        <Button variant="outline" size="sm" onClick={onClear}>
          {t("states.clearFilters")}
        </Button>
      }
    />
  ) : (
    <EmptyState title={title} />
  )
}

/** The latest action's outcome on a page; `clear` it when the next one starts. */
export function useDone(initial: string | null = null) {
  const [message, setMessage] = React.useState<string | null>(initial)
  return { message, show: setMessage, clear: () => setMessage(null) }
}

/** One persistent live region per page, so each new outcome is announced. */
export function Done({ message }: { message: string | null }) {
  return (
    <div role="status">
      {message ? (
        <Notice live={false} variant="success" title={message} />
      ) : null}
    </div>
  )
}
