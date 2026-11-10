"use client"

import { useEffect, useRef, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslations } from "next-intl"
import { ExternalLink, PanelRight, X } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import { useUsersMe } from "@genesy/api-client/endpoints/users"
import { sessionStore } from "@genesy/api-client/session"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice, type NoticeVariant } from "@genesy/ui/components/notice"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@genesy/ui/components/sheet"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { VoiceActivity, VoiceThinking } from "@genesy/voice/components/activity"
import { AttachmentPreview } from "@genesy/voice/components/attachments"
import { VoiceComposer } from "@genesy/voice/components/composer"
import { VoiceControls } from "@genesy/voice/components/controls"
import { useVoice } from "@genesy/voice/components/provider"
import {
  ReviewButtons,
  ReviewTitle,
} from "@genesy/voice/components/review-details"
import { TranscriptRows } from "@genesy/voice/components/transcript"
import { VoiceUnfinished } from "@genesy/voice/components/unfinished"
import type { TranscriptRow } from "@genesy/voice/lib/helpers"
import { useWorkspaceChoices } from "@/components/shell"
import { readinessState, type Readiness } from "@/lib/readiness"
import { ResultsList, ReviewPanel } from "./results"
import { useProposalHold } from "./storefront-proposal"
import { useWide } from "./use-wide"

const look: Record<
  Readiness,
  { variant: NoticeVariant; title: string; text?: string }
> = {
  available: { variant: "success", title: "assistantAvailable" },
  off: { variant: "info", title: "assistantOff", text: "assistantOffText" },
  not_ready: {
    variant: "warning",
    title: "assistantNotReady",
    text: "assistantNotReadyText",
  },
  unknown: { variant: "error", title: "assistantUnknown" },
}

/**
 * Talk, see results, review, act. Left: the conversation and everything that waits on the
 * person; right (wide screens) or a sheet (phones, tablets): what the assistant found.
 */
export function Conversation() {
  const t = useTranslations("flow")
  const tv = useTranslations("voice")
  const voice = useVoice()
  const wide = useWide()
  useProposalHold(voice.review)
  const me = useUsersMe()
  const workspaceId = useSession((s) => s.workspaceId)
  const { query: workspaces, choices } = useWorkspaceChoices()
  const current = choices.find((w) => w.id === workspaceId)
  const readiness = useQuery({
    queryKey: ["voice-readiness"],
    retry: false,
    queryFn: async () => {
      const response = await fetch("/api/voice/readiness", {
        cache: "no-store",
      })
      const body: unknown = await response.json().catch(() => undefined)
      return readinessState(response.status, body)
    },
  })
  const state: Readiness | undefined = readiness.isError
    ? "unknown"
    : readiness.data
  const live = voice.connected || voice.starting || voice.upgrading
  const user = me.data?.data.user

  const results = voice.messages.filter(
    (row): row is TranscriptRow => row.role === "result" && !!row.views
  )
  const [opened, setOpened] = useState(false)
  // The review whose sheet the person closed; a newer review opens it again.
  const [dismissed, setDismissed] = useState<string>()
  const [focus, setFocus] = useState<string | null>(null)
  const log = useRef<HTMLDivElement>(null)
  const [follow, setFollow] = useState(true)
  const previewReturn = useRef<HTMLElement | null>(null)
  const [previewKey, setPreviewKey] = useState<string | null>(null)

  // Keep the newest line in view while the person has not scrolled up.
  useEffect(() => {
    const el = log.current
    if (follow && el) el.scrollTop = el.scrollHeight
  }, [voice.messages, voice.thinking, voice.requesting, follow])

  // A new action to approve opens the sheet on small screens, as Office does; closing it never cancels.
  const ticket = voice.review?.ticket
  const sheet = opened || (!!ticket && !wide && dismissed !== ticket)
  const setSheet = (open: boolean) => {
    setOpened(open)
    if (!open) setDismissed(ticket)
  }

  // Bring a result into view once its pane or sheet is on screen.
  useEffect(() => {
    if (!focus || (!wide && !sheet)) return
    const timer = setTimeout(() => {
      const el = document.getElementById(`result-${focus}`)
      el?.scrollIntoView({ block: "nearest" })
      el?.focus({ preventScroll: true })
      setFocus(null)
    }, 50)
    return () => clearTimeout(timer)
  }, [focus, wide, sheet])

  const show = (id: string) => {
    setFocus(id)
    if (!wide) setSheet(true)
  }

  const handoff = voice.outcome?.handoff?.route
  const handoffHref = handoff ? voice.resolveHref(handoff) : ""

  return (
    <div className="grid h-full min-h-0 gap-5 max-md:gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
      <section
        aria-labelledby="flow-conversation-title"
        className="flex min-h-0 min-w-0 flex-col gap-3 rounded-2xl glass p-4 max-md:p-3"
      >
        <header className="flex flex-wrap items-center gap-2">
          <div className="mr-auto min-w-0">
            <h1
              id="flow-conversation-title"
              className="text-base font-semibold"
            >
              {t("assistant")}
            </h1>
            <p className="truncate text-xs text-muted-foreground">
              {user ? (
                <>
                  <span className="sr-only">{t("signedInAs")} </span>
                  {user.name}
                </>
              ) : null}
              {user && current ? " · " : null}
              {current ? (
                <>
                  <span className="sr-only">{t("workspace")} </span>
                  {current.display_name}
                </>
              ) : null}
            </p>
          </div>
          {live ? <VoiceActivity compact /> : null}
          {live ? <VoiceControls /> : null}
          {!wide ? (
            <Button
              variant="outline"
              className="min-h-11"
              onClick={() => setSheet(true)}
            >
              <PanelRight aria-hidden />
              {t("chat.results")}
              {results.length ? (
                <span className="tabular-nums">({results.length})</span>
              ) : null}
            </Button>
          ) : null}
        </header>

        {me.isError || workspaces.isError ? (
          <Notice
            variant="error"
            title={t("error")}
            action={
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  void me.refetch()
                  void workspaces.refetch()
                }}
              >
                {t("retry")}
              </Button>
            }
          />
        ) : null}

        {workspaces.isSuccess && !choices.length ? (
          <EmptyState
            title={t("noWorkspace")}
            description={t("noWorkspaceText")}
          />
        ) : workspaces.isSuccess && !current ? (
          <div className="flex flex-col gap-2">
            <h2 className="font-semibold">{t("chooseWorkspace")}</h2>
            <p className="text-sm text-muted-foreground">
              {t("chooseWorkspaceText")}
            </p>
            {choices.map((w) => (
              <Button
                key={w.id}
                variant="outline"
                className="justify-start"
                onClick={() => sessionStore.getState().selectWorkspace(w.id)}
              >
                {w.display_name}
              </Button>
            ))}
          </div>
        ) : null}

        {current ? (
          <>
            <div
              ref={log}
              role="log"
              aria-label={tv("transcript")}
              aria-live="polite"
              onScroll={() => {
                const el = log.current
                if (el)
                  setFollow(
                    el.scrollHeight - el.scrollTop - el.clientHeight < 70
                  )
              }}
              className="relative flex min-h-24 min-w-0 flex-1 flex-col gap-2 overflow-y-auto px-1"
            >
              {voice.messages.length ? (
                <TranscriptRows
                  rows={voice.messages}
                  renderResult={(row) => (
                    <Button
                      variant="outline"
                      data-role="result"
                      className="min-h-11 self-start"
                      onClick={() => show(row.id)}
                    >
                      <PanelRight aria-hidden />
                      {t(wide ? "chat.showResult" : "chat.viewResult")}
                    </Button>
                  )}
                />
              ) : (
                <div className="m-auto max-w-sm text-center">
                  <p className="font-medium">{tv("welcome")}</p>
                  <p className="mt-2 text-sm text-muted-foreground">
                    {tv("examples")}
                  </p>
                </div>
              )}
              <VoiceThinking />
            </div>
            {!follow ? (
              <Button
                variant="outline"
                className="min-h-11 self-start"
                onClick={() => {
                  log.current?.scrollTo({ top: log.current.scrollHeight })
                  setFollow(true)
                }}
              >
                {tv("newMessages")}
              </Button>
            ) : null}
            <VoiceUnfinished />

            {voice.review ? (
              <div
                role="group"
                aria-labelledby="flow-review-pending"
                className="flex flex-col gap-2 rounded-2xl border border-(--gold) p-3"
              >
                <p id="flow-review-pending" className="text-sm font-semibold">
                  {t("chat.reviewPending")}
                </p>
                <p className="text-sm">
                  <ReviewTitle review={voice.review} />
                </p>
                <div className="flex flex-wrap gap-2">
                  {wide ? (
                    <ReviewButtons />
                  ) : (
                    <Button className="min-h-11" onClick={() => setSheet(true)}>
                      {t("chat.openReview")}
                    </Button>
                  )}
                </div>
              </div>
            ) : null}

            {handoff ? (
              <Notice variant="info" title={t("chat.secureTitle")}>
                {handoffHref ? (
                  <>
                    <a
                      href={handoffHref}
                      target="_blank"
                      rel="noopener noreferrer"
                      className={buttonVariants({
                        className: "mt-2 min-h-11",
                      })}
                    >
                      {t("chat.openInOffice")}
                      <ExternalLink aria-hidden />
                    </a>
                    <p className="mt-2 text-xs">{t("chat.officeSignIn")}</p>
                  </>
                ) : (
                  t("chat.officeNotLinked")
                )}
              </Notice>
            ) : null}

            {voice.error ? (
              <p role="alert" className="text-sm text-destructive">
                {voice.error}
              </p>
            ) : null}

            {/* While an action waits for approval nothing here can be used, so nothing cancels it by accident. */}
            <div inert={Boolean(voice.review)} className="flex flex-col gap-2">
              {state && state !== "available" && !live ? (
                <Notice
                  variant={look[state].variant}
                  title={t(look[state].title)}
                  action={
                    state === "unknown" || state === "not_ready" ? (
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={readiness.isFetching}
                        onClick={() => void readiness.refetch()}
                      >
                        {t("retry")}
                      </Button>
                    ) : null
                  }
                >
                  {look[state].text ? t(look[state].text) : null}
                </Notice>
              ) : null}
              {voice.connected ? (
                <VoiceComposer
                  onPreview={(file, opener) => {
                    previewReturn.current = opener
                    setPreviewKey(file.key)
                  }}
                />
              ) : !live && state === "available" ? (
                <>
                  <VoiceControls />
                  <p className="text-center text-xs text-muted-foreground">
                    {tv("micHint")}
                  </p>
                </>
              ) : !state ? (
                <Skeleton className="h-11 w-full" />
              ) : null}
            </div>
          </>
        ) : null}
      </section>

      {wide ? (
        <section
          aria-labelledby="flow-results-title"
          className="flex min-h-0 min-w-0 flex-col gap-3 rounded-2xl glass p-4"
        >
          <h2 id="flow-results-title" className="text-base font-semibold">
            {t("chat.results")}
          </h2>
          <div className="relative flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
            <ReviewPanel withButtons={false} />
            <ResultsList rows={results} />
          </div>
        </section>
      ) : (
        <Sheet open={sheet} onOpenChange={(open) => setSheet(open)}>
          <SheetContent
            side="right"
            showCloseButton={false}
            className="w-full gap-0 p-0 sm:max-w-lg"
          >
            <div className="flex h-full min-h-0 flex-col gap-3 p-4">
              <SheetHeader className="flex-row items-center justify-between p-0">
                <SheetTitle>{t("chat.results")}</SheetTitle>
                <Button
                  variant="ghost"
                  size="icon"
                  className="min-h-11 min-w-11"
                  aria-label={t("chat.closeResults")}
                  onClick={() => setSheet(false)}
                >
                  <X aria-hidden />
                </Button>
              </SheetHeader>
              <div className="relative flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
                <ReviewPanel withButtons />
                <ResultsList rows={results} />
              </div>
            </div>
          </SheetContent>
        </Sheet>
      )}

      <AttachmentPreview
        previewKey={previewKey}
        onClose={() => setPreviewKey(null)}
        finalFocus={previewReturn}
      />
    </div>
  )
}
