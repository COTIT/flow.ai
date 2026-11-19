"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  getAccountingOpeningBaselinesListQueryKey,
  getAccountingSetupGetQueryKey,
  useAccountingActivate,
  useAccountingOpeningBaselinesList,
  useAccountingOpeningCorrectionsCreate,
  useAccountingOpeningCorrectionsReview,
  useAccountingOpeningValidate,
  useAccountingSetupGet,
} from "@genesy/api-client/endpoints/accounting"
import { useImportsList } from "@genesy/api-client/endpoints/imports"
import type { SetupResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { formatDateTime } from "@genesy/ui/lib/format"
import { DetailSkeleton } from "@/components/bento"
import { Field } from "@/components/field"
import { PageHead } from "@/components/list"
import { Done, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"
import { useIdempotencyKey } from "@/lib/idempotency"

import { AccountingTabs, isStale, useReasons } from "../shared"

type Batch = { id: string; profile: string; state: string; filename: string; created_at: string }
type Issue = { code: string; message: string }
type Review = {
  ready: boolean
  issues: Issue[]
  cutover_date: string
  opening_fingerprint: string
  setup_version: number
  inventory: { mode: string | null; quantity: string; value: string }
  receivables: { count: number; outstanding: string }
  payables: { count: number; outstanding: string }
  trial_balance: { debit: string; credit: string }
}
type Baseline = {
  id: string
  source_kind: string
  number: string
  party: string
  currency: string
  face: string
  settled: string
  outstanding: string
  corrected_outstanding: string
  corrections: number
}
type CorrectionReview = {
  current_outstanding: string
  corrected_outstanding: string
  delta: string
  review_fingerprint: string
}

const PROFILES = [
  "opening_inventory",
  "opening_receivables",
  "opening_payables",
  "opening_trial_balance",
] as const

const heading = "text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"

/** The opening: review then activate a setup book; afterwards, its balances and typed corrections. */
export default function OpeningPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const setup = useAccountingSetupGet(workspaceId, {
    query: { enabled: Boolean(workspaceId) },
  })
  const done = useDone()
  return (
    <>
      <PageHead
        eyebrow={t("shell.accounting")}
        title={t("accounting.opening.title")}
        tabs={<AccountingTabs />}
      />
      <Done message={done.message} />
      {setup.isPending ? (
        <DetailSkeleton />
      ) : setup.isError ? (
        <LoadError error={setup.error} onRetry={() => void setup.refetch()} />
      ) : setup.data.data.state === "active" ? (
        <Corrections setup={setup.data.data} done={done} />
      ) : (
        <Activation setup={setup.data.data} done={done} />
      )}
    </>
  )
}

function Activation({
  setup,
  done,
}: {
  setup: SetupResource
  done: ReturnType<typeof useDone>
}) {
  const t = useTranslations("accounting.opening")
  const common = useTranslations()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const mayActivate = useCan("accounting.activate")
  const mayReview = useCan("accounting.configure")
  // shortcut: newest 100 imports; opening batches are a handful, page when a book needs more.
  const imports = useImportsList(
    workspaceId,
    { page_size: "100" },
    { query: { enabled: Boolean(workspaceId) } }
  )
  // Newest completed batch per opening profile.
  const batches = React.useMemo(() => {
    const rows = ((imports.data?.data ?? []) as Batch[]).filter(
      (b) => b.state === "completed" && PROFILES.includes(b.profile as never)
    )
    return PROFILES.flatMap((p) => rows.find((b) => b.profile === p) ?? [])
  }, [imports.data])
  const [chosen, setChosen] = React.useState<string[] | null>(null)
  const ids = chosen ?? batches.map((b) => b.id)
  const [review, setReview] = React.useState<Review | null>(null)
  const [confirmed, setConfirmed] = React.useState(false)
  const validate = useAccountingOpeningValidate({
    mutation: {
      onSuccess: (r) => {
        setReview(r.data as unknown as Review)
        setConfirmed(false)
      },
    },
  })
  const key = useIdempotencyKey()
  const activate = useAccountingActivate({
    mutation: {
      onSuccess: () => {
        done.show(t("activated"))
        void queryClient.invalidateQueries({
          queryKey: getAccountingSetupGetQueryKey(workspaceId),
        })
      },
      onError: (e) => {
        if (isStale(e)) setReview(null)
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const runReview = () =>
    validate.mutate({
      workspace: workspaceId,
      data: { batch_ids: ids, expected_setup_version: setup.version },
    })

  if (imports.isPending) return <DetailSkeleton />
  if (imports.isError)
    return <LoadError error={imports.error} onRetry={() => void imports.refetch()} />
  return (
    <div className="flex flex-col gap-5">
      <Notice variant="info" title={t("setupInfo")} />
      {!mayReview ? <Notice variant="info" title={t("readOnly")} /> : null}
      <Card className="gap-4 px-6 py-6 max-md:p-4">
        <h2 className={heading}>{t("batches")}</h2>
        {batches.length === 0 ? (
          <Notice variant="warning" title={t("noBatches")} />
        ) : (
          <ul className="flex flex-col gap-2">
            {batches.map((b) => (
              <li key={b.id}>
                <label className="flex min-h-11 items-center gap-3">
                  <input
                    type="checkbox"
                    className="size-5"
                    checked={ids.includes(b.id)}
                    onChange={(e) => {
                      setReview(null)
                      setChosen(
                        e.target.checked ? [...ids, b.id] : ids.filter((i) => i !== b.id)
                      )
                    }}
                  />
                  <span className="min-w-0">
                    <span className="block font-medium">{t(`profiles.${b.profile}`)}</span>
                    <span className="block truncate text-sm text-muted-foreground">
                      {b.filename}
                    </span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
        )}
        {validate.error ? <ApiError error={validate.error} fallback={common("common.error")} codes={reasons} /> : null}
        {mayReview ? (
          <Button
            type="button"
            variant={review ? "outline" : "default"}
            className="self-start max-md:w-full"
            disabled={ids.length === 0 || validate.isPending}
            onClick={runReview}
          >
            {review ? t("reviewAgain") : t("review")}
          </Button>
        ) : null}
      </Card>

      {review ? (
        <Card role="region" aria-label={t("reviewTitle")} className="gap-4 px-6 py-6 max-md:p-4">
          <h2 className={heading}>{t("reviewTitle")}</h2>
          <dl className="grid grid-cols-2 gap-x-4 gap-y-2 max-md:grid-cols-1">
            <Fact label={t("cutover")} value={review.cutover_date} />
            <Fact
              label={t("stock")}
              value={t("stockValue", {
                quantity: review.inventory.quantity,
                value: review.inventory.value,
              })}
            />
            <Fact
              label={t("receivables")}
              value={t("open", {
                count: review.receivables.count,
                amount: review.receivables.outstanding,
              })}
            />
            <Fact
              label={t("payables")}
              value={t("open", {
                count: review.payables.count,
                amount: review.payables.outstanding,
              })}
            />
            <Fact
              label={t("trialBalance")}
              value={t("debitCredit", {
                debit: review.trial_balance.debit,
                credit: review.trial_balance.credit,
              })}
            />
          </dl>
          {review.ready ? (
            <Notice variant="success" live={false} title={t("ready")} />
          ) : (
            <Notice
              variant="warning"
              title={t("notReady", { count: review.issues.length })}
            >
              <ul className="list-disc pl-5">
                {review.issues.map((i) => (
                  <li key={i.code}>{i.message}</li>
                ))}
              </ul>
            </Notice>
          )}
          {review.ready && mayActivate ? (
            <>
              <Notice variant="warning" title={t("finalTitle")}>
                {t("final", { date: review.cutover_date })}
              </Notice>
              <label className="flex min-h-11 items-center gap-3">
                <input
                  type="checkbox"
                  className="size-5"
                  checked={confirmed}
                  onChange={(e) => setConfirmed(e.target.checked)}
                />
                <span>{t("confirm")}</span>
              </label>
              {activate.error ? <ApiError error={activate.error} fallback={common("common.error")} codes={reasons} /> : null}
              <Button
                type="button"
                className="self-start max-md:w-full"
                disabled={!confirmed || activate.isPending}
                onClick={() =>
                  activate.mutate({
                    workspace: workspaceId,
                    data: {
                      client_ref: key.request.headers["Idempotency-Key"],
                      expected_version: setup.version,
                      payload: {
                        batch_ids: ids,
                        opening_fingerprint: review.opening_fingerprint,
                      },
                    },
                  })
                }
              >
                {t("activate")}
              </Button>
            </>
          ) : review.ready ? (
            <Notice variant="info" title={t("noActivateRight")} />
          ) : null}
        </Card>
      ) : null}
    </div>
  )
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd className="font-medium tabular-nums break-words">{value}</dd>
    </div>
  )
}

function Corrections({
  setup,
  done,
}: {
  setup: SetupResource
  done: ReturnType<typeof useDone>
}) {
  const t = useTranslations("accounting.opening")
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const mayConfigure = useCan("accounting.configure")
  const mayPost = useCan("accounting.post")
  const mayCorrect = mayConfigure && mayPost
  const list = useAccountingOpeningBaselinesList(workspaceId, {
    query: { enabled: Boolean(workspaceId) },
  })
  const rows = (list.data?.data ?? []) as unknown as Baseline[]
  const [open, setOpen] = React.useState<string | null>(null)
  return (
    <div className="flex flex-col gap-5">
      <Notice
        variant="info"
        title={t("activeInfo", {
          at: setup.activated_at ? formatDateTime(setup.activated_at, locale) : "",
        })}
      />
      <Card className="gap-4 px-6 py-6 max-md:p-4">
        <h2 className={heading}>{t("balances")}</h2>
        {list.isPending ? (
          <DetailSkeleton />
        ) : list.isError ? (
          <LoadError error={list.error} onRetry={() => void list.refetch()} />
        ) : rows.length === 0 ? (
          <Notice variant="info" live={false} title={t("noBalances")} />
        ) : (
          <ul className="flex flex-col divide-y">
            {rows.map((b) => (
              <li key={b.id} className="flex flex-col gap-3 py-3">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="min-w-0">
                    <p className="font-medium">
                      {t(`kinds.${b.source_kind}`, { number: b.number })}
                    </p>
                    <p className="text-sm text-muted-foreground">{b.party || t("noParty")}</p>
                  </div>
                  <p className="text-sm tabular-nums">
                    {t("outstanding", {
                      amount: `${b.corrected_outstanding} ${b.currency}`,
                    })}
                    {b.corrections
                      ? ` · ${t("corrected", { count: b.corrections, from: b.outstanding })}`
                      : ""}
                  </p>
                </div>
                {mayCorrect ? (
                  open === b.id ? (
                    <CorrectionForm
                      baseline={b}
                      onClose={() => setOpen(null)}
                      onDone={() => {
                        setOpen(null)
                        done.show(t("correctionSaved"))
                      }}
                    />
                  ) : (
                    <Button
                      type="button"
                      variant="outline"
                      className="self-start max-md:w-full"
                      onClick={() => setOpen(b.id)}
                    >
                      {t("correct")}
                    </Button>
                  )
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  )
}

function CorrectionForm({
  baseline,
  onClose,
  onDone,
}: {
  baseline: Baseline
  onClose: () => void
  onDone: () => void
}) {
  const t = useTranslations("accounting.opening")
  const common = useTranslations()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const today = new Date().toISOString().slice(0, 10)
  const [form, setForm] = React.useState({
    accounting_date: today,
    corrected: baseline.corrected_outstanding,
    reason: "",
    evidence_ref: "",
  })
  const [review, setReview] = React.useState<CorrectionReview | null>(null)
  const payload = {
    baseline_id: baseline.id,
    accounting_date: form.accounting_date,
    corrected_nominal_basis: form.corrected,
    corrected_functional_carrying: form.corrected,
    reason: form.reason,
    evidence_ref: form.evidence_ref,
  }
  const check = useAccountingOpeningCorrectionsReview({
    mutation: { onSuccess: (r) => setReview(r.data as unknown as CorrectionReview) },
  })
  const key = useIdempotencyKey()
  const create = useAccountingOpeningCorrectionsCreate({
    mutation: {
      onSuccess: () => {
        void queryClient.invalidateQueries({
          queryKey: getAccountingOpeningBaselinesListQueryKey(workspaceId),
        })
        onDone()
      },
      onError: (e) => {
        if (isStale(e)) setReview(null)
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const errors = { ...fieldErrors(check.error), ...fieldErrors(create.error) }
  const edit = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => {
    setReview(null)
    setForm({ ...form, [k]: e.target.value })
  }
  return (
    <form
      aria-label={t("correctTitle")}
      className="flex flex-col gap-3 rounded-lg border p-4"
      onSubmit={(e) => {
        e.preventDefault()
        if (!review) {
          check.mutate({ workspace: workspaceId, data: payload })
          return
        }
        create.mutate({
          workspace: workspaceId,
          data: {
            client_ref: key.request.headers["Idempotency-Key"],
            payload: { ...payload, review_fingerprint: review.review_fingerprint },
          },
        })
      }}
    >
      <div className="grid grid-cols-2 gap-3 max-md:grid-cols-1">
        <Field id={`corrected_nominal_basis-${baseline.id}`} label={t("correctedAmount")} errors={errors.corrected_nominal_basis}>
          <Input
            name="corrected"
            inputMode="decimal"
            value={form.corrected}
            onChange={edit("corrected")}
          />
        </Field>
        <Field id={`accounting_date-${baseline.id}`} label={t("date")} errors={errors.accounting_date}>
          <Input
            name="accounting_date"
            type="date"
            value={form.accounting_date}
            onChange={edit("accounting_date")}
          />
        </Field>
        <Field id={`reason-${baseline.id}`} label={t("reason")} errors={errors.reason}>
          <Input
            name="reason"
            value={form.reason}
            onChange={edit("reason")}
          />
        </Field>
        <Field id={`evidence_ref-${baseline.id}`} label={t("evidence")} errors={errors.evidence_ref}>
          <Input
            name="evidence_ref"
            value={form.evidence_ref}
            onChange={edit("evidence_ref")}
          />
        </Field>
      </div>
      {review ? (
        <Notice variant="warning" title={t("correctionReview")}>
          {t("correctionPlan", {
            from: review.current_outstanding,
            to: review.corrected_outstanding,
            delta: review.delta,
          })}
        </Notice>
      ) : null}
      {check.error && !Object.keys(fieldErrors(check.error)).length ? (
        <ApiError error={check.error} fallback={common("common.error")} codes={reasons} />
      ) : null}
      {create.error && !Object.keys(fieldErrors(create.error)).length ? (
        <ApiError error={create.error} fallback={common("common.error")} codes={reasons} />
      ) : null}
      <div className="flex gap-3 max-md:flex-col">
        <Button type="submit" disabled={check.isPending || create.isPending}>
          {review ? t("saveCorrection") : t("reviewCorrection")}
        </Button>
        <Button type="button" variant="outline" onClick={onClose}>
          {t("cancel")}
        </Button>
      </div>
    </form>
  )
}
