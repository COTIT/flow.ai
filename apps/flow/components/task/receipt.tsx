"use client"

import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useStockWithBins } from "@genesy/api-client/endpoints/stock"
import { Button } from "@genesy/ui/components/button"
import { useVoice } from "@genesy/voice/components/provider"
import { ReviewTitle } from "@genesy/voice/components/review-details"
import type { VoiceOutcome, VoiceReview } from "@genesy/voice/lib/client-contract"
import { receiptOf, slotKey, stockAt, type ReadBack } from "@genesy/voice/lib/task"
import { useOnceIntent } from "@/lib/use-once-intent"

/**
 * What the approved action did, only from the server: previous values from the review's snapshot,
 * new values from the API's answer or a read of the record after the write. Never the proposal.
 */
export function Receipt({
  review,
  outcome,
}: {
  review: VoiceReview
  outcome: VoiceOutcome
}) {
  const t = useTranslations("task.receipt")
  const errors = useTranslations("voice.requestErrors")
  const voice = useVoice()
  const check = useOnceIntent(review.ticket)
  const workspace = useSession((s) => s.workspaceId) ?? ""
  const plan = receiptOf(review, outcome, undefined)
  const product = plan.readBack?.product ?? ""
  // Read after the write; never cached from before it.
  const read = useStockWithBins(workspace, product, {
    query: { enabled: Boolean(product && workspace), staleTime: 0, gcTime: 0 },
  })
  // A failed (re)read drops what an earlier read said: never shown as current.
  const rows = read.isError ? undefined : read.data?.data
  const readBack: ReadBack | undefined = rows
    ? Object.fromEntries(
        plan.rows.flatMap(({ slot }) =>
          slot
            ? [[slotKey(slot.warehouse, slot.bin), stockAt(rows, slot.warehouse, slot.bin)]]
            : []
        )
      )
    : undefined
  const receipt = receiptOf(review, outcome, readBack)
  const reason = outcome.reason && errors.has(outcome.reason) ? errors(outcome.reason) : null
  return (
    <section
      aria-labelledby="flow-receipt-title"
      data-receipt={receipt.status}
      className="flex min-w-0 flex-col gap-3 rounded-2xl border border-(--gold) p-4"
    >
      <h2 id="flow-receipt-title" className="text-sm font-semibold uppercase">
        {t(`title.${receipt.status}`)}
      </h2>
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm">
        <dt className="text-muted-foreground">{t("operation")}</dt>
        <dd className="break-words">
          <ReviewTitle review={review} />
        </dd>
        {receipt.record ? (
          <>
            <dt className="text-muted-foreground">{t("record")}</dt>
            <dd className="break-words">{receipt.record}</dd>
          </>
        ) : null}
        {receipt.rows.map((row, i) => (
          <div key={i} className="contents" data-receipt-row>
            <dt className="text-muted-foreground">
              {t("previous")}
              {row.label ? ` · ${row.label}` : null}
            </dt>
            <dd>{row.before ?? "—"}</dd>
            <dt className="text-muted-foreground">{t("new")}</dt>
            <dd data-receipt-new>
              {row.after ??
                (receipt.readBack && (read.isError || readBack) ? (
                  <span className="flex flex-wrap items-center gap-2">
                    {/* Read but that slot was not in it: unknown, never an invented value. */}
                    {t(read.isError ? "readFailed" : "unconfirmed")}
                    <Button
                      variant="outline"
                      size="sm"
                      className="min-h-11"
                      onClick={() => void read.refetch()}
                    >
                      {t("refresh")}
                    </Button>
                  </span>
                ) : receipt.readBack ? (
                  t("reading")
                ) : (
                  "—"
                ))}
            </dd>
          </div>
        ))}
        <dt className="text-muted-foreground">{t("status")}</dt>
        <dd>{t(`statuses.${receipt.status}`)}</dd>
        {receipt.reference ? (
          <>
            <dt className="text-muted-foreground">{t("reference")}</dt>
            <dd className="font-mono text-xs break-all">{receipt.reference}</dd>
          </>
        ) : null}
      </dl>
      {reason ? <p className="text-sm">{reason}</p> : null}
      {receipt.status === "unknown" ? (
        <div className="flex flex-wrap items-center gap-2">
          <p className="mr-auto text-sm">{t("unknownText")}</p>
          {/* A status question to Genesy; the write itself is never sent again. */}
          {voice.connected ? (
            <Button
              variant="outline"
              className="min-h-11"
              disabled={check.pending}
              aria-busy={check.pending}
              onClick={() => void check.send(t("checkMessage"))}
            >
              {t("check")}
            </Button>
          ) : null}
        </div>
      ) : null}
      {outcome.links?.length ? (
        <nav aria-label={t("next")} className="flex flex-wrap gap-2">
          {outcome.links.map((link) => (
            <a
              key={link.href}
              href={voice.resolveHref(link.href)}
              target="_blank"
              rel="noopener noreferrer"
              className="text-sm underline"
            >
              {link.label ?? link.href}
            </a>
          ))}
        </nav>
      ) : null}
    </section>
  )
}

