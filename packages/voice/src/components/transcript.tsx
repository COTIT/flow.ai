"use client"

import { Fragment, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { ExternalLink } from "lucide-react"
import { splitLinks, type TranscriptRow } from "../lib/helpers"
import { useVoice } from "./provider"
import { VoiceResult } from "./results/index"

/**
 * The conversation lines: user and agent bubbles, action notes, record links and result cards.
 * Agent-text links go through the host's resolveHref; one it refuses ("") stays plain text.
 */
export function TranscriptRows({
  rows,
  renderResult,
}: {
  rows: TranscriptRow[]
  /** Draws a result row in place of the shared card (a host that shows results elsewhere). */
  renderResult?: (row: TranscriptRow) => ReactNode
}) {
  const t = useTranslations("voice")
  const { resolveHref } = useVoice()
  return rows.map((row) =>
    // voice-rich-results: server-built picture for a read result.
    row.role === "result" && row.views ? (
      renderResult ? (
        <Fragment key={row.id}>{renderResult(row)}</Fragment>
      ) : (
        <VoiceResult key={row.id} views={row.views} />
      )
    ) : row.role === "link" && row.href ? (
      <a
        key={row.id}
        data-role="link"
        href={row.href}
        target="_blank"
        rel="noopener noreferrer"
        aria-label={t("openRecordNamed", { name: row.text })}
        className="inline-flex min-h-11 max-w-[80%] items-center gap-2 self-start rounded-2xl rounded-bl-md border border-(--gold) px-4 py-2 text-sm font-medium break-all text-foreground underline-offset-4 transition-transform duration-100 hover:underline focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none active:scale-[0.97] motion-reduce:transition-none"
      >
        {row.text}
        <ExternalLink aria-hidden className="size-4 shrink-0" />
      </a>
    ) : (
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
        <span className="sr-only">{t(`roles.${row.role}`)}: </span>
        {row.role === "agent"
          ? splitLinks(row.text).map((part, i) => {
              const href = part.href ? resolveHref(part.href) : ""
              return href ? (
                <a
                  key={i}
                  href={href}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="font-medium underline underline-offset-4"
                >
                  {part.text}
                </a>
              ) : (
                part.text
              )
            })
          : row.text}
      </div>
    )
  )
}
