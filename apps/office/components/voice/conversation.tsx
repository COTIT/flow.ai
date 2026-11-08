"use client"

import Link from "next/link"
import { useEffect, useMemo, useRef, useState } from "react"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"
import {
  Camera,
  ClipboardList,
  FileText,
  Paperclip,
  Send,
  Trash2,
  X,
} from "lucide-react"
import AnimatedList from "@/components/rb/AnimatedList"
import PromptBar from "@/components/rb/PromptBar"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@genesy/ui/components/dialog"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetFooter,
} from "@genesy/ui/components/sheet"
import { VoiceActivity, VoiceThinking } from "./activity"
import {
  displayReferences,
  displayText,
  composerReady,
  internalFields,
  operationLabel,
  reviewRows,
  type DisplayReferences,
} from "./presentation"
import type { VoiceReview, VoiceReviewStep } from "@/lib/voice/client-contract"
import { ATTACHMENT_ACCEPT, referenceSize } from "./helpers"
import { useVoice } from "./provider"
import { HoldToTalk, VoiceControls } from "./controls"
import { StrandsStatus } from "./strands-status"

function ReviewTitle({
  review,
}: {
  review: Pick<VoiceReviewStep, "summary" | "operationId" | "method">
}) {
  const t = useTranslations("voice")
  if (!review.operationId)
    return (
      <>
        {displayText(
          review.summary,
          displayReferences(review.summary),
          (number) => t("recordReference", { number })
        )}
      </>
    )
  const { action, resource } = operationLabel(review.operationId, review.method)
  return (
    <>
      {t(`operationActions.${action}`, { record: t(`resources.${resource}`) })}
    </>
  )
}

/** The review card: every thing the agent will do, one row each, popping in one after another. */
function ReviewList({ review }: { review: VoiceReview }) {
  const labels = useTranslations("voice.reviewFields")
  // The list measures its rows on mount; mount it once the sheet has slid into view.
  const [ready, setReady] = useState(false)
  useEffect(() => {
    const timer = setTimeout(() => setReady(true), 320)
    return () => clearTimeout(timer)
  }, [review.ticket])
  // Same array identity per review: a new array restarts the list's reveal animation.
  const rows = useMemo(() => {
    const references = displayReferences(review.details)
    return reviewRows(review).map((row) =>
      row.kind === "step"
        ? {
            id: row.id,
            icon: <ClipboardList aria-hidden className="size-4" />,
            title: (
              <>
                {row.number}. <ReviewTitle review={row.step} />
              </>
            ),
            description: <Details value={row.step.details} />,
          }
        : {
            id: row.id,
            title: labels.has(row.key) ? labels(row.key) : row.key,
            description: (
              <Details
                value={row.value}
                references={references}
                field={row.key}
              />
            ),
          }
    )
  }, [review, labels])
  if (!rows.length) return <Details value={review.details} />
  if (!ready) return null
  return (
    <AnimatedList
      items={rows}
      animation="pop"
      stagger={0.07}
      selectOnHover
      enableArrowNavigation={false}
      frame={false}
      showGradients={false}
      displayScrollbar={false}
      theme="dark"
      accentColor="var(--gold)"
      radius={14}
      gap={8}
      width="100%"
      // Unbounded: the sheet body scrolls, so no row hides behind a list with no scrollbar.
      maxHeight="none"
      // The list never scrolls itself; as a scroll box it would swallow the wheel meant for the sheet.
      className="voice-review-list [&_[role=listbox]]:overflow-visible [&_[role=listbox]]:overscroll-auto"
    />
  )
}

function Details({
  value,
  references: supplied,
  field,
}: {
  value: unknown
  references?: DisplayReferences
  field?: string
}) {
  const t = useTranslations("voice")
  const labels = useTranslations("voice.reviewFields")
  const references = supplied ?? displayReferences(value)
  if (field === "workspace") return <span>{t("currentWorkspace")}</span>
  if (value == null) return <span>—</span>
  if (
    typeof value === "string" &&
    (/^\/(products|orders|invoices|lots|stock|supply|till|settings)(?:\/|\?|$)/.test(
      value
    ) ||
      /^https:\/\//.test(value))
  )
    return (
      <a
        className="inline-flex min-h-11 max-w-full items-center font-medium break-all underline underline-offset-4"
        href={value}
        target={value.startsWith("https:") ? "_blank" : undefined}
        rel="noreferrer noopener"
      >
        {value.startsWith("https:")
          ? displayText(value, references, (number) =>
              t("recordReference", { number })
            )
          : t("openRecord")}
      </a>
    )
  if (typeof value !== "object") {
    const text =
      typeof value === "boolean"
        ? t(value ? "yes" : "no")
        : typeof value === "string" &&
            ["status", "state", "kind", "send_method"].includes(field ?? "") &&
            t.has(`values.${value}`)
          ? t(`values.${value}`)
          : displayText(String(value), references, (number) =>
              t("recordReference", { number })
            )
    return <span className="break-words whitespace-pre-wrap">{text}</span>
  }
  if (Array.isArray(value))
    return (
      <ol className="flex flex-col gap-3">
        {value.map((item, i) => (
          <li key={i}>
            <Details value={item} references={references} field={field} />
          </li>
        ))}
      </ol>
    )
  const taskMetadata = "actionId" in value && "summary" in value
  return (
    <dl className="flex min-w-0 flex-col gap-3">
      {Object.entries(value)
        .filter(
          ([key]) =>
            !internalFields.has(key) &&
            !(taskMetadata && key === "operation") &&
            !key.endsWith("_sha256") &&
            !key.endsWith("_hash")
        )
        .map(([key, item]) => (
          <div key={key} className="min-w-0">
            <dt className="text-xs text-muted-foreground">
              {labels.has(key)
                ? labels(key)
                : t("otherField", {
                    name: key
                      .replaceAll("_", " ")
                      .replace(/([a-z0-9])([A-Z])/g, "$1 $2"),
                  })}
            </dt>
            <dd className="text-sm">
              {key === "summary" &&
              taskMetadata &&
              typeof (value as Record<string, unknown>).operation ===
                "string" ? (
                <ReviewTitle
                  review={{
                    summary: String(item),
                    operationId: String(
                      (value as Record<string, unknown>).operation
                    ),
                  }}
                />
              ) : (
                <Details value={item} references={references} field={key} />
              )}
            </dd>
          </div>
        ))}
    </dl>
  )
}
export function VoiceConversation() {
  const t = useTranslations("voice")
  const locale = useLocale()
  const router = useRouter()
  const voice = useVoice()
  const [follow, setFollow] = useState(true)
  const log = useRef<HTMLDivElement>(null)
  const picker = useRef<HTMLInputElement>(null)
  const camera = useRef<HTMLInputElement>(null)
  const previewReturn = useRef<HTMLElement | null>(null)
  const [previewKey, setPreviewKey] = useState<string | null>(null)
  const [previewUnavailable, setPreviewUnavailable] = useState(false)
  const preview = voice.attachments.find((file) => file.key === previewKey)
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
        <div className="theme-glass flex h-full min-h-0 w-full flex-col">
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
                  voice.messages.map((row) => (
                    <div
                      key={row.id}
                      data-role={row.role}
                      className={
                        row.role === "action"
                          ? "max-w-[65ch] self-center rounded-full border border-(--hair) px-3 py-1 text-center text-xs text-muted-foreground"
                          : row.role === "user"
                            ? "max-w-[80%] self-end rounded-2xl rounded-br-md bg-(--gold) px-4 py-2 text-sm leading-relaxed break-words whitespace-pre-wrap text-black"
                            : "max-w-[80%] self-start rounded-2xl rounded-bl-md border border-(--hair) bg-card px-4 py-2 text-sm leading-relaxed break-words whitespace-pre-wrap"
                      }
                    >
                      <span className="sr-only">
                        {t(`roles.${row.role}`)}:{" "}
                      </span>
                      {row.text}
                    </div>
                  ))
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
                <form
                  className="voice-composer flex min-w-0 flex-col gap-2"
                  onSubmit={(e) => e.preventDefault()}
                  onKeyDown={(e) => {
                    // Escape that closed the composer's menu must not close the assistant too.
                    if (e.key === "Escape" && e.defaultPrevented)
                      e.stopPropagation()
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
                      placeholder={t("placeholder")}
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
                    <p className="text-sm text-muted-foreground">
                      {t("micHint")}
                    </p>
                  )}
                  {voice.capturePaused ? (
                    <Notice variant="info" title={t("capturePaused")}>
                      {t("capturePausedHint")}
                    </Notice>
                  ) : null}
                  {voice.attachments.length ? (
                    <ul
                      aria-label={t("attachments")}
                      className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2"
                    >
                      {voice.attachments.map((file) => (
                        <li key={file.key} className="flex min-w-0 gap-3 py-2">
                          <Button
                            className="size-14 min-h-11 min-w-11 shrink-0 p-0"
                            variant="ghost"
                            type="button"
                            aria-label={t("previewReference", {
                              number: file.ordinal,
                            })}
                            onClick={(event) => {
                              previewReturn.current = event.currentTarget
                              setPreviewUnavailable(false)
                              setPreviewKey(file.key)
                            }}
                          >
                            {file.preview &&
                            file.file.type.startsWith("image/") ? (
                              <img
                                src={file.preview}
                                alt={t("preview", { number: file.ordinal })}
                                className="size-14 shrink-0 rounded-xl object-cover"
                              />
                            ) : (
                              <FileText
                                aria-hidden
                                className="size-10 shrink-0"
                              />
                            )}
                          </Button>
                          <div className="min-w-0 flex-1">
                            <p className="truncate text-sm" title={file.name}>
                              {file.ordinal}. {file.name}
                            </p>
                            <p className="text-xs text-muted-foreground">
                              {referenceSize(file.file.size, locale)}
                            </p>
                            <p
                              role="status"
                              className="text-xs text-muted-foreground"
                            >
                              {t(`fileStates.${file.state}`)}
                            </p>
                            {file.error ? (
                              <p
                                role="alert"
                                className="text-xs text-destructive"
                              >
                                {file.error}
                              </p>
                            ) : null}
                            {file.state === "failed" ? (
                              <Button
                                className="mt-1 min-h-11"
                                type="button"
                                variant="outline"
                                disabled={voice.busy}
                                aria-label={t("retryReference", {
                                  number: file.ordinal,
                                })}
                                onClick={() =>
                                  void voice.retryAttachment(file.key)
                                }
                              >
                                {t("retry")}
                              </Button>
                            ) : null}
                          </div>
                          {file.state !== "submitted" ? (
                            <Button
                              className="min-h-11 min-w-11"
                              size="icon"
                              type="button"
                              variant="ghost"
                              disabled={voice.busy}
                              aria-label={t("removeReference", {
                                name: file.name,
                                number: file.ordinal,
                              })}
                              onClick={() => void voice.remove(file.key)}
                            >
                              <Trash2 aria-hidden />
                            </Button>
                          ) : null}
                        </li>
                      ))}
                    </ul>
                  ) : null}
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
                      <span className="sr-only sm:not-sr-only">
                        {t("takePhoto")}
                      </span>
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
                    <p
                      id="voice-file-hint"
                      className="text-xs text-muted-foreground"
                    >
                      {t("fileHint")}
                    </p>
                  ) : null}
                </form>
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
      <Dialog
        open={Boolean(preview)}
        onOpenChange={(open) => {
          if (!open) setPreviewKey(null)
        }}
      >
        <DialogContent showCloseButton={false} finalFocus={previewReturn}>
          <DialogHeader>
            <DialogTitle className="break-words">{preview?.name}</DialogTitle>
          </DialogHeader>
          {preview ? (
            <p className="text-sm text-muted-foreground">
              {referenceSize(preview.file.size, locale)} · {preview.file.type}
            </p>
          ) : null}
          {preview?.file.type.startsWith("image/") && !previewUnavailable ? (
            <img
              src={preview.preview}
              alt={preview.name}
              className="max-h-[60dvh] w-full rounded-xl object-contain"
              onError={() => setPreviewUnavailable(true)}
            />
          ) : preview?.file.type === "application/pdf" ? (
            <>
              <p className="text-sm text-muted-foreground">
                {t("pdfPreviewHint")}
              </p>
              <a
                className="inline-flex min-h-11 items-center text-sm font-medium underline underline-offset-4"
                href={preview.preview}
                target="_blank"
                rel="noopener noreferrer"
              >
                {t("openPdf")}
              </a>
            </>
          ) : (
            <Notice variant="error" title={t("previewUnavailable")} />
          )}
          <DialogFooter>
            <Button className="min-h-11" onClick={() => setPreviewKey(null)}>
              {t("closePreview")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Sheet
        open={Boolean(voice.review)}
        onOpenChange={(open) => {
          if (!open && !voice.busy) void voice.confirm("cancel")
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
              {voice.review ? <ReviewList review={voice.review} /> : null}
            </div>
            <p className="text-xs text-muted-foreground">{t("reviewHint")}</p>
            {voice.recoveredReview ? (
              <p className="text-sm text-muted-foreground">
                {t("recoveredReviewHint")}
              </p>
            ) : null}
            {voice.review?.confirmationPhrase && !voice.recoveredReview ? (
              <p className="text-sm">
                {t("spokenHint")}{" "}
                <strong className="font-semibold break-words">
                  {voice.review.confirmationPhrase}
                </strong>
              </p>
            ) : null}
            <SheetFooter className="p-0">
              <Button
                className="min-h-11"
                variant="outline"
                disabled={voice.busy && voice.review?.kind !== "batch"}
                onClick={() =>
                  voice.busy && voice.review?.kind === "batch"
                    ? void voice.stopBatch("cancel")
                    : void voice.confirm("cancel")
                }
              >
                {t(
                  voice.busy && voice.review?.kind === "batch"
                    ? "batchStop"
                    : "cancel"
                )}
              </Button>
              <Button
                className="min-h-11"
                disabled={voice.busy}
                onClick={() => void voice.confirm("approve")}
              >
                {t(voice.busy ? "working" : "approve")}
              </Button>
            </SheetFooter>
          </div>
        </SheetContent>
      </Sheet>
    </Dialog>
  )
}
