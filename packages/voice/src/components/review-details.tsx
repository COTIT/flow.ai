"use client"

import { useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { ClipboardList } from "lucide-react"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import AnimatedList from "./rb/AnimatedList"
import {
  displayReferences,
  displayText,
  internalFields,
  operationLabel,
  reviewRows,
  notIncludedRows,
  type DisplayReferences,
} from "../lib/presentation"
import type { VoiceReview, VoiceReviewStep } from "../lib/client-contract"
import { fieldName } from "../lib/helpers"
import { useVoice } from "./provider"

export function ReviewTitle({
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

/** Rows kept out of the review: the person fixes them in the file, nothing is guessed. */
export function NotIncluded({ review }: { review: VoiceReview }) {
  const t = useTranslations("voice")
  const rows = notIncludedRows(review)
  const range = review.details.rows as
    { from: number; to: number; total: number } | undefined
  const remaining = review.details.remaining
  const more = Number.isSafeInteger(remaining) ? (remaining as number) : 0
  if (!rows.length && !range && !more) return null
  return (
    <div className="mt-3 grid gap-2">
      {more ? (
        <Notice variant="info" title={t("moreParts", { count: more })} />
      ) : null}
      {range ? (
        <p className="text-xs text-muted-foreground">
          {t("importRows", range)}
        </p>
      ) : null}
      {rows.length ? (
        <Notice variant="warning" title={t("notIncluded.title")}>
          <ul className="grid gap-1">
            {rows.map((row, index) => (
              <li key={index} className="break-words">
                {row.line ? t("notIncluded.line", { line: row.line }) : null}
                {row.text} — {t(`notIncluded.reasons.${row.reason}`)}
                {row.note ? ` (${row.note})` : null}
              </li>
            ))}
          </ul>
        </Notice>
      ) : null}
    </div>
  )
}

/** The review card: every thing the agent will do, one row each, popping in one after another. */
export function ReviewList({ review }: { review: VoiceReview }) {
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
            description: <ReviewDetails value={row.step.details} />,
          }
        : {
            id: row.id,
            title: labels.has(row.key) ? labels(row.key) : row.key,
            description: (
              <ReviewDetails
                value={row.value}
                references={references}
                field={row.key}
              />
            ),
          }
    )
  }, [review, labels])
  if (!rows.length) return <ReviewDetails value={review.details} />
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

const LINKED =
  /^\/(products|orders|invoices|lots|stock|supply|till|settings)(?:\/|\?|$)/

export function ReviewDetails({
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
  const { resolveHref } = useVoice()
  const references = supplied ?? displayReferences(value)
  if (field === "workspace") return <span>{t("currentWorkspace")}</span>
  if (value == null) return <span>—</span>
  const href =
    typeof value === "string" &&
    (LINKED.test(value) || /^https:\/\//.test(value))
      ? resolveHref(value)
      : ""
  if (typeof value === "string" && href)
    return (
      <a
        className="inline-flex min-h-11 max-w-full items-center font-medium break-all underline underline-offset-4"
        href={href}
        target={/^https?:/.test(href) ? "_blank" : undefined}
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
            <ReviewDetails value={item} references={references} field={field} />
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
                : t("otherField", { name: fieldName(key) })}
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
                <ReviewDetails
                  value={item}
                  references={references}
                  field={key}
                />
              )}
            </dd>
          </div>
        ))}
    </dl>
  )
}

/** What approving means and, when the agent can hear it, the phrase that approves by voice. */
export function ReviewHints() {
  const t = useTranslations("voice")
  const voice = useVoice()
  return (
    <>
      <p className="text-xs text-muted-foreground">{t("reviewHint")}</p>
      {voice.recoveredReview ? (
        <p className="text-sm text-muted-foreground">
          {t("recoveredReviewHint")}
        </p>
      ) : null}
      {/* A held approval is not offered by voice either. */}
      {voice.review?.confirmationPhrase && !voice.recoveredReview && !voice.approvalHeld ? (
        <p className="text-sm">
          {t("spokenHint")}{" "}
          <strong className="font-semibold break-words">
            {voice.review.confirmationPhrase}
          </strong>
        </p>
      ) : null}
    </>
  )
}

/** Cancel (or stop a running batch) and approve; both wait while a decision is on its way. */
export function ReviewButtons() {
  const t = useTranslations("voice")
  const voice = useVoice()
  const batchRunning = voice.busy && voice.review?.kind === "batch"
  return (
    <>
      <Button
        className="min-h-11"
        variant="outline"
        disabled={voice.busy && voice.review?.kind !== "batch"}
        onClick={() =>
          batchRunning
            ? void voice.stopBatch("cancel")
            : void voice.confirm("cancel")
        }
      >
        {t(batchRunning ? "batchStop" : "cancel")}
      </Button>
      <Button
        className="min-h-11"
        disabled={voice.busy || voice.approvalHeld}
        onClick={() => void voice.confirm("approve")}
      >
        {t(voice.busy ? "working" : "approve")}
      </Button>
    </>
  )
}
