"use client"

import { useEffect, useRef, useState } from "react"
import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { ExternalLink, Mic, MicOff, Paperclip, Send } from "lucide-react"

import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Notice, type NoticeVariant } from "@genesy/ui/components/notice"
import {
  AttachmentList,
  AttachmentPreview,
} from "@genesy/voice/components/attachments"
import { useVoice } from "@genesy/voice/components/provider"
import { ReviewTitle } from "@genesy/voice/components/review-details"
import { VoiceUnfinished } from "@genesy/voice/components/unfinished"
import { ATTACHMENT_ACCEPT } from "@genesy/voice/lib/helpers"
import type { SessionPhase } from "@genesy/voice/lib/presentation"
import { flowStore, useFlow } from "@/lib/flow-store"
import { usePills } from "@/lib/pills"
import type { Readiness } from "@/lib/readiness"
import { useReadiness } from "@/lib/use-readiness"
import { parseWake } from "@/lib/wake"

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
 * The same place on every screen: suggestions, what needs saying, then type / attach / talk / send.
 * Typing never opens the microphone; the unsent text survives navigation.
 */
export function Dock({ phase }: { phase: SessionPhase["phase"] }) {
  const t = useTranslations()
  const voice = useVoice()
  const router = useRouter()
  const pathname = usePathname()
  const pills = usePills()
  const draft = useFlow((s) => s.draft)
  const resultOpen = useFlow((s) => s.resultOpen)
  const pending = useRef<string | null>(null)
  const picker = useRef<HTMLInputElement>(null)
  const previewReturn = useRef<HTMLElement | null>(null)
  const [previewKey, setPreviewKey] = useState<string | null>(null)
  const live = phase === "live"
  const { query: readiness, state } = useReadiness()

  const deliver = async (text: string) => {
    if (await voice.send(text)) flowStore.getState().setDraft("")
  }
  // Text typed before the conversation was up goes out once it connects.
  useEffect(() => {
    if (!voice.connected || pending.current === null) return
    const text = pending.current
    pending.current = null
    if (text) void deliver(text)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [voice.connected])

  // Text sent while readiness is still being checked starts the conversation once it is known.
  const waiting = useRef(false)
  useEffect(() => {
    if (!waiting.current || !state) return
    waiting.current = false
    if (state === "available") void voice.start(true)
    else pending.current = null
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state])

  const submit = () => {
    const { wake, rest } = parseWake(draft.trim())
    const files = voice.attachments.some((file) => file.state !== "submitted")
    if (!rest && !wake && !files) return
    if (!voice.connected) {
      if (voice.starting || voice.upgrading || waiting.current) return
      if (state && state !== "available") return
      pending.current = rest
      if (!rest) flowStore.getState().setDraft("")
      if (state) void voice.start(true)
      else waiting.current = true
      return
    }
    if (!rest && !files) return flowStore.getState().setDraft("")
    void deliver(rest)
  }

  const handoff = voice.outcome?.handoff?.route
  const handoffHref = handoff ? voice.resolveHref(handoff) : ""
  return (
    <section
      aria-label={t("shell.dock")}
      className="flex min-w-0 shrink-0 flex-col gap-2 rounded-2xl glass-strong p-3"
    >
      {pills.length ? (
        <ul
          aria-label={t("shell.suggested")}
          className="flex gap-2 overflow-x-auto"
        >
          {pills.map((pill) => (
            <li key={pill.id} className="shrink-0">
              <Button variant="outline" size="sm" onClick={pill.run}>
                {pill.label}
              </Button>
            </li>
          ))}
        </ul>
      ) : null}

      {state && state !== "available" && !live ? (
        <Notice
          variant={look[state].variant}
          title={t(`flow.${look[state].title}`)}
          action={
            state === "unknown" || state === "not_ready" ? (
              <Button
                variant="outline"
                size="sm"
                disabled={readiness.isFetching}
                onClick={() => void readiness.refetch()}
              >
                {t("flow.retry")}
              </Button>
            ) : null
          }
        >
          {look[state].text ? t(`flow.${look[state].text}`) : null}
        </Notice>
      ) : null}

      {phase === "ended" ? (
        <p role="status" className="text-sm">
          {t("shell.ended")}{" "}
          {voice.unfinished || voice.recoveredTasks.length
            ? `${t("shell.preserved")} `
            : null}
          <span className="text-muted-foreground">{t("shell.startAgain")}</span>
        </p>
      ) : null}
      <VoiceUnfinished />

      {voice.review && !(resultOpen && pathname === "/") ? (
        <div
          role="group"
          aria-labelledby="flow-review-pending"
          className="flex flex-wrap items-center gap-2 rounded-xl border border-(--gold) p-2"
        >
          <p id="flow-review-pending" className="mr-auto text-sm">
            <span className="font-semibold">{t("flow.chat.reviewPending")}</span>
            {": "}
            <ReviewTitle review={voice.review} />
          </p>
          <Button
            className="min-h-11"
            onClick={() => {
              flowStore.setState({ resultOpen: true })
              if (pathname !== "/") router.push("/")
            }}
          >
            {t("flow.chat.openReview")}
          </Button>
        </div>
      ) : null}

      {handoff ? (
        <Notice variant="info" title={t("flow.chat.secureTitle")}>
          {handoffHref ? (
            <>
              <a
                href={handoffHref}
                target="_blank"
                rel="noopener noreferrer"
                className={buttonVariants({ className: "mt-2 min-h-11" })}
              >
                {t("flow.chat.openInOffice")}
                <ExternalLink aria-hidden />
              </a>
              <p className="mt-2 text-xs">{t("flow.chat.officeSignIn")}</p>
            </>
          ) : (
            t("flow.chat.officeNotLinked")
          )}
        </Notice>
      ) : null}

      {voice.error ? (
        <p role="alert" className="text-sm text-destructive">
          {voice.error}
        </p>
      ) : null}

      {voice.connected ? (
        <AttachmentList
          onPreview={(file, opener) => {
            previewReturn.current = opener
            setPreviewKey(file.key)
          }}
        />
      ) : null}

      {/* While an action waits for approval nothing here can be used, so nothing cancels it by accident. */}
      <form
        inert={Boolean(voice.review)}
        className="flex min-w-0 items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault()
          submit()
        }}
        onPaste={(e) => {
          if (!voice.connected || !e.clipboardData.files.length) return
          e.preventDefault()
          voice.choose(Array.from(e.clipboardData.files))
        }}
      >
        {voice.connected ? (
          <>
            <input
              ref={picker}
              type="file"
              multiple
              accept={ATTACHMENT_ACCEPT}
              aria-label={t("voice.chooseFiles")}
              hidden
              onChange={(e) => {
                voice.choose(Array.from(e.target.files ?? []))
                e.target.value = ""
              }}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="min-h-11 min-w-11"
              aria-label={t("voice.addFiles")}
              onClick={() => picker.current?.click()}
            >
              <Paperclip aria-hidden />
            </Button>
          </>
        ) : null}
        <Input
          aria-label={t("voice.message")}
          placeholder={t("shell.placeholder")}
          autoComplete="off"
          value={draft}
          onChange={(e) => {
            flowStore.getState().setDraft(e.target.value)
            if (voice.connected) voice.activity()
          }}
          className="min-h-11 min-w-0 flex-1"
        />
        {!live ? (
          <Button
            type="button"
            variant="outline"
            size="icon"
            className="min-h-11 min-w-11"
            aria-label={t("voice.start")}
            disabled={state !== "available"}
            onClick={() => void voice.start(false)}
          >
            <Mic aria-hidden />
          </Button>
        ) : voice.connected && voice.textOnly ? (
          <Button
            type="button"
            variant="outline"
            size="icon"
            className="min-h-11 min-w-11"
            aria-label={t("shell.switchVoice")}
            disabled={voice.busy || Boolean(voice.review)}
            onClick={() => void voice.upgrade()}
          >
            <Mic aria-hidden />
          </Button>
        ) : voice.connected ? (
          <Button
            type="button"
            variant="outline"
            size="icon"
            className="min-h-11 min-w-11"
            aria-label={t("shell.mic")}
            aria-pressed={!voice.muted}
            onClick={voice.toggleMute}
          >
            {voice.muted ? <MicOff aria-hidden /> : <Mic aria-hidden />}
          </Button>
        ) : null}
        <Button
          type="submit"
          size="icon"
          className="min-h-11 min-w-11"
          aria-label={t("voice.send")}
          disabled={!live && state !== "available"}
        >
          <Send aria-hidden />
        </Button>
      </form>
      <AttachmentPreview
        previewKey={previewKey}
        onClose={() => setPreviewKey(null)}
        finalFocus={previewReturn}
      />
    </section>
  )
}
