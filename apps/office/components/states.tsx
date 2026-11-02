"use client"

import * as React from "react"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"

/** A list or page that failed to load: the API's words (or a plain fallback) and a retry. */
export function LoadError({
  error,
  onRetry,
}: {
  error: unknown
  onRetry: () => void
}) {
  const t = useTranslations()
  const message = (
    error as { response?: { data?: { message?: string } } } | null
  )?.response?.data?.message
  return (
    <Notice
      variant="error"
      title={message ?? t("common.error")}
      action={
        <Button variant="outline" size="sm" onClick={onRetry}>
          {t("states.retry")}
        </Button>
      }
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
