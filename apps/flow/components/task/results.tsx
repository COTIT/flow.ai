"use client"

import { useTranslations } from "next-intl"

import type { TranscriptRow } from "@genesy/voice/lib/helpers"
import { useVoice } from "@genesy/voice/components/provider"
import { VoiceResult } from "@genesy/voice/components/results/index"
import { ReviewDiff } from "@genesy/voice/components/review-diff"
import {
  NotIncluded,
  ReviewButtons,
  ReviewHints,
  ReviewList,
  ReviewTitle,
} from "@genesy/voice/components/review-details"
import { useTakeFocus } from "@/lib/use-take-focus"
import { designKind, StorefrontProposal } from "./storefront-proposal"

/** The results of this conversation, newest first; each can be focused from its transcript line. */
export function ResultsList({ rows }: { rows: TranscriptRow[] }) {
  // P5: answers show inline; this screen is only "Show all", so it never shows an empty box.
  if (!rows.length) return null
  return (
    <ol className="flex flex-col gap-3">
      {[...rows].reverse().map((row) => (
        <li
          key={row.id}
          id={`result-${row.id}`}
          tabIndex={-1}
          className="flex min-w-0 flex-col rounded-2xl outline-none focus-visible:ring-2 focus-visible:ring-(--gold)"
        >
          <VoiceResult views={row.views ?? []} />
        </li>
      ))}
    </ol>
  )
}

/** The pending action in full: what changes (before → after), every detail, and what approving means. */
export function ReviewPanel({ withButtons }: { withButtons: boolean }) {
  const t = useTranslations("voice")
  const voice = useVoice()
  const heading = useTakeFocus<HTMLHeadingElement>(voice.review?.ticket)
  if (!voice.review) return null
  const design = designKind(voice.review)
  return (
    <section
      aria-labelledby="flow-review-title"
      className="flex min-w-0 flex-col gap-3 rounded-2xl border border-(--gold) p-4"
    >
      <h3 ref={heading} tabIndex={-1} id="flow-review-title" className="text-sm font-semibold">
        {t("review")}: <ReviewTitle review={voice.review} />
      </h3>
      <ReviewDiff review={voice.review}>
        <ReviewList review={voice.review} />
      </ReviewDiff>
      {/* A design review also shows the page it leads to; a new ticket mounts a fresh frame. */}
      {design ? (
        <StorefrontProposal key={voice.review.ticket} review={voice.review} kind={design} />
      ) : null}
      <NotIncluded review={voice.review} />
      <ReviewHints />
      {withButtons ? (
        <div data-review-buttons className="flex flex-wrap justify-end gap-2">
          <ReviewButtons />
        </div>
      ) : null}
    </section>
  )
}
