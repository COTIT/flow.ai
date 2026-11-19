"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import {
  tillShiftCountQuote,
  useTillCashAdjustmentsCreate,
  useTillCashAdjustmentsCreateReview,
} from "@verzo/api-client/endpoints/till"
import type {
  AdjustReviewResource,
  CashAdjustmentResource,
  CountLine,
} from "@verzo/api-client/model"
import { useIdempotencyKey } from "@verzo/api-client/idempotency"
import { Input } from "@verzo/ui/components/input"
import { Label } from "@verzo/ui/components/label"
import { Notice } from "@verzo/ui/components/notice"
import { ApprovalGate } from "@/components/approval-gate"
import { CountForm } from "@/components/count-form"
import { Refused } from "@/components/refused"
import { privileged, useApproval } from "@/lib/approval"
import { useTill } from "@/lib/till-store"
import { useTillMoney } from "@/lib/use-money"

/**
 * Drawer cash adjustment (privileged, `cashregister.adjust`): the cashier counts the drawer, a
 * member approves, the API reviews counted against expected and books the difference.
 */
export default function AdjustPage() {
  const t = useTranslations("adjust")
  return (
    <main className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
      <Link
        href="/drawer"
        prefetch={false}
        className="badge-link w-fit underline"
      >
        {t("back")}
      </Link>
      <ApprovalGate right="cashregister.adjust">
        <Adjust />
      </ApprovalGate>
    </main>
  )
}

function Adjust() {
  const t = useTranslations("adjust")
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  const register = useTill((s) => s.session?.register.id) ?? ""
  const cash = useTill((s) => s.session?.register.accepted_cash?.join())
  const approval = useApproval()
  const idem = useIdempotencyKey()
  const { money } = useTillMoney()
  const [evidence, setEvidence] = React.useState("")
  const [reason, setReason] = React.useState("")
  const [review, setReview] = React.useState<{
    count: string
    data: AdjustReviewResource
  } | null>(null)
  const [done, setDone] = React.useState<CashAdjustmentResource | null>(null)
  const [error, setError] = React.useState<unknown>(null)
  const auth = privileged(approval)
  const preview = useTillCashAdjustmentsCreateReview({ request: auth })
  const post = useTillCashAdjustmentsCreate({
    request: { headers: { ...auth.headers, ...idem.request.headers } },
    mutation: { onSettled: (_, e) => idem.settle(e) },
  })
  const place = { kind: "drawer", id: register }

  const submit = async (count: CountLine[]) => {
    setError(null)
    const key = JSON.stringify(count)
    try {
      if (review && review.count === key) {
        const { data } = await post.mutateAsync({
          workspace,
          data: {
            client_ref: idem.request.headers["Idempotency-Key"],
            payload: {
              target_cash_place: place,
              counted_amount: review.data.counted,
              count_evidence_ref: evidence.trim() || null,
              reason: reason.trim(),
              review_fingerprint: review.data.review_fingerprint,
            },
          },
        })
        setDone(data)
        return
      }
      // The API adds the count up; the till sends its total to the review as counted.
      const { data: quote } = await tillShiftCountQuote(workspace, { count })
      const { data } = await preview.mutateAsync({
        workspace,
        data: { target_cash_place: place, counted_amount: quote.total },
      })
      setReview({ count: key, data })
    } catch (e) {
      setError(e)
    }
  }

  if (done)
    return (
      <Notice
        variant="success"
        title={t("done", { amount: money(done.difference) })}
      >
        {t("doneBody", { counted: money(done.counted) })}
      </Notice>
    )

  return (
    <CountForm
      key={`adjust-${cash}`}
      title={t("title")}
      sub={t("hint")}
      badge={<span className="tbadge gold">{t("badge")}</span>}
      aside={
        <>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="adjust-evidence">{t("evidence")}</Label>
            <Input
              id="adjust-evidence"
              className="h-12"
              maxLength={255}
              required
              value={evidence}
              onChange={(e) => setEvidence(e.target.value)}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="adjust-reason">{t("reason")}</Label>
            <Input
              id="adjust-reason"
              className="h-12"
              maxLength={1000}
              value={reason}
              onChange={(e) => {
                setReview(null)
                setReason(e.target.value)
              }}
            />
          </div>
          {review ? (
            <div
              className="till-card strong tight"
              role="region"
              aria-label={t("reviewTitle")}
            >
              {(
                [
                  ["expected", review.data.expected],
                  ["counted", review.data.counted],
                  ["difference", review.data.difference],
                ] as const
              ).map(([k, v]) => (
                <div
                  key={k}
                  className="flex items-center justify-between gap-3"
                >
                  <span id={`adjust-${k}`} className="text-muted-foreground">
                    {t(k)}
                  </span>
                  <output
                    aria-labelledby={`adjust-${k}`}
                    className="font-semibold tabular-nums"
                  >
                    {money(v)}
                  </output>
                </div>
              ))}
            </div>
          ) : null}
          {error ? <Refused error={error} fallback={t("refused")} /> : null}
        </>
      }
      submitLabel={review ? t("post") : t("review")}
      pending={preview.isPending || post.isPending}
      onSubmit={(count) => void submit(count)}
    />
  )
}
