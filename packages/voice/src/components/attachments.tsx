"use client"

import { useState, type RefObject } from "react"
import { useLocale, useTranslations } from "next-intl"
import { FileSpreadsheet, FileText, Trash2 } from "lucide-react"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@genesy/ui/components/dialog"
import { referenceSize, sheetMime } from "../lib/helpers"
import { useVoice, type Attachment } from "./provider"

/** The chosen reference files: thumbnail (opens the preview), name, size, state, retry and remove. */
export function AttachmentList({
  onPreview,
}: {
  onPreview: (file: Attachment, opener: HTMLElement) => void
}) {
  const t = useTranslations("voice")
  const locale = useLocale()
  const voice = useVoice()
  if (!voice.attachments.length) return null
  // Narrow or short screens get one sideways row of chips, so ten files never push the
  // composer off a phone (P4-2); roomy screens (at least 40rem each way) keep the grid.
  return (
    <ul
      aria-label={t("attachments")}
      className="flex min-w-0 gap-2 overflow-x-auto overscroll-x-contain [@media(min-width:40rem)_and_(min-height:40rem)]:grid [@media(min-width:40rem)_and_(min-height:40rem)]:grid-cols-2 [@media(min-width:40rem)_and_(min-height:40rem)]:overflow-visible"
    >
      {voice.attachments.map((file) => (
        <li
          key={file.key}
          className="flex w-64 min-w-0 shrink-0 gap-3 py-1 [@media(min-width:40rem)_and_(min-height:40rem)]:w-auto [@media(min-width:40rem)_and_(min-height:40rem)]:py-2"
        >
          <Button
            className="size-11 min-h-11 min-w-11 shrink-0 p-0 [@media(min-width:40rem)_and_(min-height:40rem)]:size-14"
            variant="ghost"
            type="button"
            aria-label={t("previewReference", {
              number: file.ordinal,
            })}
            onClick={(event) => onPreview(file, event.currentTarget)}
          >
            {file.preview && file.file.type.startsWith("image/") ? (
              <img
                src={file.preview}
                alt={t("preview", { number: file.ordinal })}
                className="size-11 shrink-0 rounded-xl object-cover [@media(min-width:40rem)_and_(min-height:40rem)]:size-14"
              />
            ) : sheetMime(file.file) ? (
              <FileSpreadsheet aria-hidden className="size-10 shrink-0" />
            ) : (
              <FileText aria-hidden className="size-10 shrink-0" />
            )}
          </Button>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm" title={file.name}>
              {file.ordinal}. {file.name}
            </p>
            {/* One line in a chip, two in the grid. */}
            <div className="flex gap-2 text-xs text-muted-foreground [@media(min-width:40rem)_and_(min-height:40rem)]:block">
              <p>{referenceSize(file.file.size, locale)}</p>
              <p role="status">{t(`fileStates.${file.state}`)}</p>
            </div>
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
              onClick={(event) => {
                // Move focus before this button leaves the DOM: the dialog would otherwise
                // pull it onto its popup a frame later, away from the tray.
                const row = event.currentTarget.closest("li")
                ;(row?.nextElementSibling ?? row?.previousElementSibling)
                  ?.querySelector("button")
                  ?.focus()
                void voice.remove(file.key)
              }}
            >
              <Trash2 aria-hidden />
            </Button>
          ) : null}
        </li>
      ))}
    </ul>
  )
}

/** One reference file in a dialog: the image, a PDF link, or why there is no preview. */
export function AttachmentPreview({
  previewKey,
  onClose,
  finalFocus,
}: {
  previewKey: string | null
  onClose: () => void
  finalFocus: RefObject<HTMLElement | null>
}) {
  const t = useTranslations("voice")
  const locale = useLocale()
  const voice = useVoice()
  const preview = voice.attachments.find((file) => file.key === previewKey)
  const [unavailable, setUnavailable] = useState<string | null>(null)
  const previewUnavailable = unavailable === previewKey
  return (
    <Dialog
      open={Boolean(preview)}
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent showCloseButton={false} finalFocus={finalFocus}>
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
            onError={() => setUnavailable(previewKey)}
          />
        ) : preview && sheetMime(preview.file) ? (
          <Notice variant="info" title={t("sheetNoPreview")} />
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
          <Button className="min-h-11" onClick={onClose}>
            {t("closePreview")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
