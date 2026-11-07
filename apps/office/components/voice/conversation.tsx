"use client"

import Link from "next/link"
import { useRef, useState } from "react"
import { useLocale, useTranslations } from "next-intl"
import { Camera, FileText, Paperclip, Send, Trash2 } from "lucide-react"
import { Button } from "@genesy/ui/components/button"
import { Textarea } from "@genesy/ui/components/textarea"
import { Notice } from "@genesy/ui/components/notice"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@genesy/ui/components/dialog"
import { Bento, SubTile, Tile } from "@/components/bento"
import { PageHead } from "@/components/list"
import { ATTACHMENT_ACCEPT, referenceSize, taskPresentation } from "./helpers"
import { useVoice } from "./provider"
import { VoiceControls } from "./controls"
import { StrandsStatus } from "./strands-status"
import type { BatchProgress } from "./continuity"

function Details({ value }: { value: unknown }) {
  const labels = useTranslations("voice.reviewFields")
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
        className="inline-flex min-h-11 items-center font-medium break-all underline underline-offset-4"
        href={value}
        target={value.startsWith("https:") ? "_blank" : undefined}
        rel="noreferrer noopener"
      >
        {value}
      </a>
    )
  if (typeof value !== "object")
    return (
      <span className="break-words whitespace-pre-wrap">{String(value)}</span>
    )
  if (Array.isArray(value))
    return (
      <ol className="flex flex-col gap-3">
        {value.map((item, i) => (
          <li key={i}>
            <Details value={item} />
          </li>
        ))}
      </ol>
    )
  return (
    <dl className="flex min-w-0 flex-col gap-3">
      {Object.entries(value).map(([key, item]) => (
        <div key={key} className="min-w-0">
          <dt className="text-xs text-muted-foreground capitalize">
            {labels.has(key) ? labels(key) : key.replaceAll("_", " ")}
          </dt>
          <dd className="text-sm">
            <Details value={item} />
          </dd>
        </div>
      ))}
    </dl>
  )
}
function BatchSteps({
  steps,
  state,
}: {
  steps: BatchProgress["steps"]
  state?: string
}) {
  const t = useTranslations("voice")
  return (
    <ol className="flex min-w-0 flex-col gap-2">
      {steps.map((step, index) => (
        <li
          key={step.data?.actionId ?? index}
          className="rounded-xl bg-(--glass-subtle) p-3"
        >
          <p className="text-sm font-medium break-words">
            {index + 1}. {step.data?.summary ?? t("currentTask")}
          </p>
          <p className="mt-1 text-sm" role="status">
            {t(`taskStates.${taskPresentation(step.status, state)}`)}
          </p>
          {step.message ? (
            <p className="mt-1 text-sm text-muted-foreground">{step.message}</p>
          ) : null}
        </li>
      ))}
    </ol>
  )
}

export function VoiceConversation() {
  const t = useTranslations("voice")
  const locale = useLocale()
  const voice = useVoice()
  const [draft, setDraft] = useState("")
  const [follow, setFollow] = useState(true)
  const log = useRef<HTMLDivElement>(null)
  const picker = useRef<HTMLInputElement>(null)
  const camera = useRef<HTMLInputElement>(null)
  const previewReturn = useRef<HTMLElement | null>(null)
  const [previewKey, setPreviewKey] = useState<string | null>(null)
  const [previewUnavailable, setPreviewUnavailable] = useState(false)
  const preview = voice.attachments.find((file) => file.key === previewKey)
  const staged = voice.attachments.some((file) => file.state !== "submitted")
  const send = async () => {
    if (await voice.send(draft)) setDraft("")
  }
  return (
    <>
      <PageHead
        eyebrow={t("eyebrow")}
        title={t("title")}
        description={t("description")}
      />
      <Bento>
        <Tile
          id="voice-conversation"
          title={t("conversation")}
          span="col-span-8"
          action={<VoiceControls />}
        >
          <StrandsStatus />
          <div
            ref={log}
            role="log"
            aria-label={t("transcript")}
            aria-live="polite"
            onScroll={() => {
              const el = log.current
              if (el)
                setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 70)
            }}
            className="flex h-[320px] min-w-0 flex-col gap-4 overflow-y-auto rounded-2xl bg-(--glass-subtle) p-4 md:h-[420px]"
          >
            {voice.messages.length ? (
              voice.messages.map((row) => (
                <div
                  key={row.id}
                  className={row.role === "user" ? "ml-6" : "mr-6"}
                >
                  <p className="mb-1 text-xs font-semibold text-muted-foreground">
                    {t(`roles.${row.role}`)}
                  </p>
                  <p className="text-sm leading-relaxed break-words whitespace-pre-wrap">
                    {row.text}
                  </p>
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
            <div
              ref={(element) => {
                if (follow && element && voice.messages.length)
                  element.scrollIntoView({ block: "nearest" })
              }}
            />
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
              className="flex min-w-0 flex-col gap-3 pb-[env(safe-area-inset-bottom)]"
              onSubmit={(e) => {
                e.preventDefault()
                void send()
              }}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault()
                voice.choose(Array.from(e.dataTransfer.files))
              }}
            >
              <label htmlFor="voice-message" className="text-sm font-medium">
                {t("message")}
              </label>
              <Textarea
                id="voice-message"
                rows={2}
                value={draft}
                onChange={(e) => {
                  setDraft(e.target.value)
                  voice.activity()
                }}
                onPaste={(e) => {
                  if (e.clipboardData.files.length) {
                    e.preventDefault()
                    voice.choose(Array.from(e.clipboardData.files))
                  }
                }}
                placeholder={t("placeholder")}
                aria-describedby="voice-file-hint"
              />
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
                    <li
                      key={file.key}
                      className="flex min-w-0 gap-3 rounded-2xl border border-(--hair) bg-(--glass-subtle) p-3"
                    >
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
                        {file.preview && file.file.type.startsWith("image/") ? (
                          <img
                            src={file.preview}
                            alt={t("preview", { number: file.ordinal })}
                            className="size-14 shrink-0 rounded-xl object-cover"
                          />
                        ) : (
                          <FileText aria-hidden className="size-10 shrink-0" />
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
                          <p role="alert" className="text-xs text-destructive">
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
                            onClick={() => void voice.retryAttachment(file.key)}
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
                    voice.choose(Array.from(e.target.files ?? []))
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
                  onClick={() => picker.current?.click()}
                >
                  <Paperclip aria-hidden />
                  {t("attach")}
                </Button>
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
                  {t("takePhoto")}
                </Button>
                <Button
                  className="min-h-11"
                  type="submit"
                  disabled={voice.busy || (!draft.trim() && !staged)}
                >
                  <Send aria-hidden />
                  {t(
                    voice.busy
                      ? "working"
                      : voice.attachments.some((f) => f.state === "failed")
                        ? "retry"
                        : "send"
                  )}
                </Button>
              </div>
              <p id="voice-file-hint" className="text-xs text-muted-foreground">
                {t("fileHint")}
              </p>
            </form>
          ) : (
            <p className="text-sm text-muted-foreground">{t("micHint")}</p>
          )}
        </Tile>
        <Tile id="voice-task" title={t("currentTask")} span="col-span-4">
          <SubTile>
            <p className="text-sm">{t("controlHint")}</p>
            <p className="mt-2 text-xs text-muted-foreground">
              {t("privacyHint")}
            </p>
          </SubTile>
          {voice.outcome ? (
            <Notice
              variant={
                voice.outcome.status === "completed"
                  ? "success"
                  : voice.outcome.status === "failed"
                    ? "error"
                    : "warning"
              }
              title={
                voice.outcome.handoff
                  ? t("secureTitle")
                  : t(`taskStates.${taskPresentation(voice.outcome.status)}`)
              }
            >
              <Details value={voice.outcome.data} />
            </Notice>
          ) : null}
          {voice.outcome?.handoff?.route.startsWith("/voice/secure?") ? (
            <Link
              className="inline-flex min-h-11 items-center text-sm font-medium underline"
              href={voice.outcome.handoff.route}
            >
              {t("secureOpen")}
            </Link>
          ) : null}
          {!voice.outcome ? (
            <p className="text-sm text-muted-foreground">{t("taskEmpty")}</p>
          ) : null}
          {voice.batchProgress ? (
            <section
              aria-label={t("batchProgress")}
              className="flex min-w-0 flex-col gap-3"
            >
              <h2 className="text-sm font-semibold">{t("batchProgress")}</h2>
              {voice.batchProgress.summary ? (
                <p className="text-sm">{voice.batchProgress.summary}</p>
              ) : null}
              <BatchSteps
                steps={voice.batchProgress.steps}
                state={voice.batchProgress.state}
              />
              <Button
                className="min-h-11 self-start"
                variant="outline"
                onClick={() => void voice.refreshBatch()}
              >
                {t("recoveryRefresh")}
              </Button>
            </section>
          ) : null}
          {voice.recovery && !voice.connected && !voice.starting ? (
            <section
              aria-label={t("recoveryTitle")}
              className="flex min-w-0 flex-col gap-3 border-t border-(--hair) pt-4"
            >
              <h2 className="text-sm font-semibold">{t("recoveryTitle")}</h2>
              <p className="text-sm text-muted-foreground">
                {t("recoveryHint")}
              </p>
              {voice.checkingRecovery ? (
                <p role="status" className="text-sm">
                  {t("recoveryLoading")}
                </p>
              ) : null}
              {voice.recoveredTasks.length ? (
                <ul className="flex min-w-0 flex-col gap-3">
                  {voice.recoveredTasks.map((task, index) => (
                    <li
                      key={task.data?.batchId ?? task.data?.actionId ?? index}
                      className="min-w-0 rounded-xl bg-(--glass-subtle) p-3"
                    >
                      <p className="text-sm font-medium break-words">
                        {task.data?.summary ?? t("currentTask")}
                      </p>
                      <p className="mt-1 text-sm" role="status">
                        {t(`taskStates.${taskPresentation(task.status)}`)}
                      </p>
                      {task.message ? (
                        <p className="mt-2 text-sm text-muted-foreground">
                          {task.message}
                        </p>
                      ) : null}
                      {task.data?.kind === "batch" && task.data.steps ? (
                        <BatchSteps
                          steps={task.data.steps}
                          state={task.data.state}
                        />
                      ) : null}
                      {task.data?.recoverable &&
                      (task.data?.batchId || task.data?.actionId) ? (
                        <Button
                          className="mt-2 min-h-11"
                          variant="outline"
                          disabled={voice.busy || voice.checkingRecovery}
                          onClick={() =>
                            void voice.restore(
                              (task.data!.batchId ?? task.data!.actionId)!,
                              task.data?.kind === "batch" ? "batch" : "action"
                            )
                          }
                        >
                          {t("recoveryReview")}
                        </Button>
                      ) : null}
                    </li>
                  ))}
                </ul>
              ) : !voice.checkingRecovery ? (
                <p className="text-sm text-muted-foreground">
                  {t("recoveryEmpty")}
                </p>
              ) : null}
              <div className="flex flex-wrap gap-2">
                <Button
                  className="min-h-11"
                  variant="outline"
                  disabled={voice.checkingRecovery || voice.busy}
                  onClick={() => void voice.refreshRecovery()}
                >
                  {t("recoveryRefresh")}
                </Button>
                <Button
                  className="min-h-11"
                  variant="ghost"
                  disabled={voice.checkingRecovery || voice.busy}
                  onClick={() => void voice.closeRecovery()}
                >
                  {t("recoveryClose")}
                </Button>
              </div>
            </section>
          ) : null}
        </Tile>
      </Bento>
      {voice.error ? <Notice variant="error" title={voice.error} /> : null}
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
      <Dialog
        open={Boolean(voice.review)}
        onOpenChange={(open) => {
          if (!open && !voice.busy) void voice.confirm("cancel")
        }}
      >
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>{t("review")}</DialogTitle>
          </DialogHeader>
          <p className="text-sm font-semibold">{voice.review?.summary}</p>
          {voice.review?.target ? (
            <p className="text-sm break-words">{voice.review.target}</p>
          ) : null}
          <div className="max-h-[45dvh] overflow-y-auto">
            <Details value={voice.review?.details} />
            {voice.review?.kind === "batch" ? (
              <ol
                aria-label={t("batchSteps")}
                className="mt-4 flex flex-col gap-4"
              >
                {voice.review.steps?.map((step, index) => (
                  <li
                    key={step.actionId}
                    className="rounded-xl border border-(--hair) p-3"
                  >
                    <h3 className="mb-2 text-sm font-semibold">
                      {index + 1}. {step.summary}
                    </h3>
                    {step.target ? (
                      <p className="mb-2 text-sm break-words">{step.target}</p>
                    ) : null}
                    <Details value={step.details} />
                  </li>
                ))}
              </ol>
            ) : null}
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
          <DialogFooter>
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
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
