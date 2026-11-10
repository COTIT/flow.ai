"use client"

import { useRef } from "react"
import { useTranslations } from "next-intl"
import { Camera, Paperclip, Send } from "lucide-react"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import PromptBar from "./rb/PromptBar"
import { composerReady } from "../lib/presentation"
import { ATTACHMENT_ACCEPT } from "../lib/helpers"
import { AttachmentList } from "./attachments"
import { HoldToTalk } from "./controls"
import { useVoice, type Attachment } from "./provider"

/**
 * What the person types, says or attaches in a connected session: the text box (text mode) or
 * hold-to-talk, chosen reference files, the file picker and camera, and the file hint.
 */
export function VoiceComposer({
  seed,
  placeholder,
  onPreview,
}: {
  /** Text a suggestion put in the box; a new object refills it. */
  seed?: { text: string }
  placeholder?: string
  onPreview: (file: Attachment, opener: HTMLElement) => void
}) {
  const t = useTranslations("voice")
  const voice = useVoice()
  const picker = useRef<HTMLInputElement>(null)
  const camera = useRef<HTMLInputElement>(null)
  return (
    <form
      className="voice-composer flex min-w-0 flex-col gap-2"
      onSubmit={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        // Escape that closed the composer's menu must not close the assistant too.
        if (e.key === "Escape" && e.defaultPrevented) e.stopPropagation()
      }}
      onPaste={(e) => {
        if (!e.clipboardData.files.length) return
        e.preventDefault()
        voice.choose(Array.from(e.clipboardData.files))
      }}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault()
        voice.choose(Array.from(e.dataTransfer.files))
      }}
    >
      {voice.textOnly ? (
        <PromptBar
          labels={{
            prompt: t("message"),
            add: t("addFiles"),
            sources: t("addFiles"),
            send: t("send"),
            stop: t("stopSpeaking"),
          }}
          ready={composerReady(voice.attachments)}
          describedBy="voice-file-hint"
          placeholder={placeholder ?? t("placeholder")}
          seed={seed}
          sources={[
            {
              key: "files",
              name: t("attach"),
              description: t("fileHint"),
              icon: Paperclip,
              attach: true,
            },
          ]}
          commands={[]}
          models={[]}
          efforts={[]}
          busy={voice.speaking}
          width={768}
          radius={18}
          background="color-mix(in srgb, var(--card) 85%, transparent)"
          color="var(--foreground)"
          menuBackground="var(--card)"
          sparkColor="var(--gold)"
          onSend={(text) => {
            voice.activity()
            return voice.send(text)
          }}
          onStop={() => voice.stopSpeaking()}
          // Chosen files show in our references list, not as bar chips.
          onAttach={() => picker.current?.click()}
          className="w-full"
        />
      ) : voice.micMode === "hold" && !voice.capturePaused ? (
        <HoldToTalk />
      ) : (
        <p className="text-sm text-muted-foreground">{t("micHint")}</p>
      )}
      {voice.capturePaused ? (
        <Notice variant="info" title={t("capturePaused")}>
          {t("capturePausedHint")}
        </Notice>
      ) : null}
      <AttachmentList onPreview={onPreview} />
      <div className="flex flex-wrap gap-2">
        <input
          ref={picker}
          type="file"
          multiple
          accept={ATTACHMENT_ACCEPT}
          aria-label={t("chooseFiles")}
          className="hidden"
          tabIndex={-1}
          onChange={(e) => {
            const files = Array.from(e.target.files ?? [])
            voice.choose(files)
            e.target.value = ""
          }}
        />
        <input
          ref={camera}
          type="file"
          accept="image/jpeg,image/png,image/webp"
          capture="environment"
          aria-label={t("takePhoto")}
          className="hidden"
          tabIndex={-1}
          onChange={(e) => {
            voice.choose(Array.from(e.target.files ?? []))
            e.target.value = ""
          }}
        />
        <Button
          className="min-h-11"
          type="button"
          variant="outline"
          disabled={voice.busy}
          onClick={() => {
            voice.pauseCapture()
            camera.current?.click()
          }}
        >
          <Camera aria-hidden />
          <span className="sr-only sm:not-sr-only">{t("takePhoto")}</span>
        </Button>
        {!voice.textOnly && composerReady(voice.attachments) ? (
          <Button
            className="min-h-11"
            type="button"
            disabled={voice.busy}
            onClick={() => void voice.send("")}
          >
            <Send aria-hidden />
            {t(voice.busy ? "working" : "send")}
          </Button>
        ) : null}
      </div>
      {voice.textOnly ? (
        <p id="voice-file-hint" className="text-xs text-muted-foreground">
          {t("fileHint")}
        </p>
      ) : null}
    </form>
  )
}
