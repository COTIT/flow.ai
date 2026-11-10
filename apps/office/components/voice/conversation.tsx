"use client"

import Link from "next/link"
import { useEffect, useRef, useState } from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { X } from "lucide-react"
import { Button } from "@genesy/ui/components/button"
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from "@genesy/ui/components/dialog"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetFooter,
} from "@genesy/ui/components/sheet"
import { VoiceActivity, VoiceThinking } from "@genesy/voice/components/activity"
import { useVoice } from "@genesy/voice/components/provider"
import { VoiceControls } from "@genesy/voice/components/controls"
import { StrandsStatus } from "@genesy/voice/components/strands-status"
import { VoiceUnfinished } from "@genesy/voice/components/unfinished"
import { ReviewDiff } from "@genesy/voice/components/review-diff"
import {
  NotIncluded,
  ReviewButtons,
  ReviewHints,
  ReviewList,
  ReviewTitle,
} from "@genesy/voice/components/review-details"
import { TranscriptRows } from "@genesy/voice/components/transcript"
import { AttachmentPreview } from "@genesy/voice/components/attachments"
import { VoiceComposer } from "@genesy/voice/components/composer"
import {
  useVoiceTips,
  VoiceSuggestions,
} from "@genesy/voice/components/suggestions"
import type { Available } from "@genesy/voice/lib/capabilities"
import { useRights } from "@/lib/can"

export function VoiceConversation({
  capabilities = [],
  from,
}: {
  capabilities?: Available[]
  from?: string
}) {
  const t = useTranslations("voice")
  const router = useRouter()
  const voice = useVoice()
  // Suggestion chips fill the text box (starting a text session if needed); they never send.
  const rights = useRights()
  const tips = useVoiceTips(capabilities, from, rights)
  const [seed, setSeed] = useState<{ text: string }>()
  const pick = (text: string) => {
    setSeed({ text })
    if (!voice.connected && !voice.starting) void voice.start(true)
  }
  const [follow, setFollow] = useState(true)
  const log = useRef<HTMLDivElement>(null)
  const previewReturn = useRef<HTMLElement | null>(null)
  const [previewKey, setPreviewKey] = useState<string | null>(null)
  // The thinking row is part of the log too; keep it in view when it appears or grows.
  useEffect(() => {
    const transcript = log.current
    if (follow && transcript) transcript.scrollTop = transcript.scrollHeight
  }, [voice.messages, voice.thinking, voice.requesting, voice.steps, follow])
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !(voice.busy && voice.review)) {
          void (async () => {
            if (voice.review) await voice.confirm("cancel")
            router.push("/")
          })()
        }
      }}
    >
      <DialogContent
        data-voice-overlay=""
        showCloseButton={false}
        overlayClassName="bg-black/75 motion-reduce:animate-none"
        className="dark top-0 left-0 translate-x-0 translate-y-0 text-foreground motion-reduce:animate-none"
      >
        {/* While a review card is open nothing behind it can be reached, so nothing cancels it by accident. */}
        <div
          inert={Boolean(voice.review)}
          className="theme-glass flex h-full min-h-0 w-full flex-col"
        >
          <header className="flex shrink-0 items-center justify-between gap-2 px-4 py-3 md:px-8">
            <DialogTitle className="min-w-0 flex-1 truncate text-base">
              {t("title")}
            </DialogTitle>
            {voice.connected || voice.starting || voice.upgrading ? (
              <VoiceControls />
            ) : null}
            <Button
              variant="ghost"
              size="icon"
              className="min-h-11 min-w-11"
              aria-label={t("closeAssistant")}
              disabled={Boolean(voice.busy && voice.review)}
              onClick={() => {
                void (async () => {
                  if (voice.review) await voice.confirm("cancel")
                  router.push("/")
                })()
              }}
            >
              <X aria-hidden />
            </Button>
          </header>
          <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-4 pb-[max(1rem,env(safe-area-inset-bottom))] md:px-8">
            <div className="flex min-h-44 flex-[1_0_auto] flex-col items-center justify-center gap-4 py-4">
              <StrandsStatus />
              <VoiceActivity />
              {!voice.connected && !voice.starting && !voice.upgrading ? (
                <VoiceControls />
              ) : null}
            </div>
            <div
              id="voice-conversation"
              className="mx-auto flex w-full max-w-3xl shrink-0 flex-col gap-3"
            >
              <div
                ref={log}
                role="log"
                aria-label={t("transcript")}
                aria-live="polite"
                onScroll={() => {
                  const el = log.current
                  if (el)
                    setFollow(
                      el.scrollHeight - el.scrollTop - el.clientHeight < 70
                    )
                }}
                className="flex max-h-[32dvh] min-h-12 min-w-0 flex-col gap-2 overflow-y-auto px-1"
              >
                {voice.messages.length ? (
                  <TranscriptRows rows={voice.messages} />
                ) : (
                  <div className="m-auto max-w-sm text-center">
                    <p className="font-medium">{t("welcome")}</p>
                    <p className="mt-2 text-sm text-muted-foreground">
                      {t("examples")}
                    </p>
                  </div>
                )}
                <VoiceThinking />
              </div>
              <VoiceUnfinished />
              <VoiceSuggestions items={tips.chips} onPick={pick} />
              {!follow ? (
                <Button
                  variant="outline"
                  className="min-h-11 self-start"
                  onClick={() => {
                    log.current?.scrollTo({ top: log.current.scrollHeight })
                    setFollow(true)
                  }}
                >
                  {t("newMessages")}
                </Button>
              ) : null}
              {voice.connected ? (
                <VoiceComposer
                  seed={seed}
                  placeholder={tips.placeholder}
                  onPreview={(file, opener) => {
                    previewReturn.current = opener
                    setPreviewKey(file.key)
                  }}
                />
              ) : (
                <p className="text-sm text-muted-foreground">{t("micHint")}</p>
              )}
            </div>
            {voice.outcome?.handoff?.route.startsWith("/voice/secure?") ? (
              <Link
                className="mx-auto mt-3 inline-flex min-h-11 w-full max-w-3xl items-center text-sm font-medium underline"
                href={voice.outcome.handoff.route}
              >
                {t("secureOpen")}
              </Link>
            ) : null}
            {voice.error ? (
              <p
                role="alert"
                className="mx-auto mt-3 w-full max-w-3xl text-sm text-destructive"
              >
                {voice.error}
              </p>
            ) : null}
          </div>
        </div>
      </DialogContent>
      <AttachmentPreview
        previewKey={previewKey}
        onClose={() => setPreviewKey(null)}
        finalFocus={previewReturn}
      />
      <Sheet
        open={Boolean(voice.review)}
        onOpenChange={(open, details) => {
          // Only Esc cancels here; focus loss, resizes and stray clicks never do.
          if (!open && !voice.busy && details.reason === "escape-key")
            void voice.confirm("cancel")
        }}
      >
        <SheetContent
          side="right"
          showCloseButton={false}
          className="dark w-full gap-0 overflow-hidden bg-background p-0 text-foreground motion-reduce:transition-none sm:max-w-md"
        >
          <div className="theme-glass flex h-full min-h-0 flex-col gap-4 p-5">
            <SheetHeader className="p-0">
              <SheetTitle>{t("review")}</SheetTitle>
            </SheetHeader>
            <p className="text-sm font-semibold">
              {voice.review ? <ReviewTitle review={voice.review} /> : null}
            </p>
            <div className="min-h-0 flex-1 overflow-y-auto">
              {/* voice-rich-results: before → after diff on top, the rest under "More details". */}
              {voice.review ? (
                <ReviewDiff review={voice.review}>
                  <ReviewList review={voice.review} />
                </ReviewDiff>
              ) : null}
              {voice.review ? <NotIncluded review={voice.review} /> : null}
            </div>
            <ReviewHints />
            <SheetFooter className="p-0">
              <ReviewButtons />
            </SheetFooter>
          </div>
        </SheetContent>
      </Sheet>
    </Dialog>
  )
}
