"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import { useTranslations } from "next-intl"
import { Trash2 } from "lucide-react"

import { useSession } from "@verzo/api-client/auth"
import {
  getAccountingJournalsGetQueryKey,
  getAccountingJournalsListQueryKey,
  useAccountingJournalsCreate,
  useAccountingJournalsUpdate,
} from "@verzo/api-client/endpoints/accounting"
import { useCurrenciesList } from "@verzo/api-client/endpoints/currencies"
import type { JournalPayload, JournalResource } from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { Card } from "@verzo/ui/components/card"
import { Input } from "@verzo/ui/components/input"
import { Textarea } from "@verzo/ui/components/textarea"
import { Field } from "@/components/field"
import { AccountPicker, CurrencyPicker } from "@/components/pickers"
import { useIdempotencyKey } from "@/lib/idempotency"

import { isStale, today, useReasons } from "../shared"

type Line = {
  key: string
  account_id: string
  debit: string
  credit: string
  source_ref: string
}

const blankLine = (): Line => ({
  key: crypto.randomUUID(),
  account_id: "",
  debit: "",
  credit: "",
  source_ref: "",
})

/** Amounts the API sends for the empty side come back as "0"; the form shows them blank. */
const shown = (amount: string) => (/^0(\.0+)?$/.test(amount) ? "" : amount)

/**
 * A manual journal draft (Form card, as the purchase order form): create, or full replace.
 * Amounts go as typed; the API checks balance and totals when the draft is posted.
 */
export function JournalForm({ journal }: { journal?: JournalResource }) {
  const t = useTranslations()
  const router = useRouter()
  const reasons = useReasons()
  const queryClient = useQueryClient()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  // The workspace default currency is preselected for a new journal (first page holds it).
  const currencies = useCurrenciesList(workspaceId, { page_size: 100 }, enabled)
  const [currency, setCurrency] = React.useState(journal?.currency.id ?? "")
  const key = useIdempotencyKey()
  const [lines, setLines] = React.useState<Line[]>(() =>
    journal?.lines.length
      ? journal.lines.map((l) => ({
          key: `${l.line_no}`,
          account_id: l.account_id,
          debit: shown(l.debit),
          credit: shown(l.credit),
          source_ref: l.source_ref ?? "",
        }))
      : [blankLine(), blankLine()]
  )
  const refresh = (id: string) => {
    for (const queryKey of [
      getAccountingJournalsListQueryKey(workspaceId),
      getAccountingJournalsGetQueryKey(workspaceId, id),
    ])
      void queryClient.invalidateQueries({ queryKey })
  }
  const done = (id: string, message: "created" | "saved") => {
    refresh(id)
    router.push(`/accounting/journals/${id}?done=${message}`)
  }
  const create = useAccountingJournalsCreate({
    mutation: {
      onSuccess: (r) => done(r.data.id, "created"),
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const update = useAccountingJournalsUpdate({
    mutation: {
      onSuccess: (r) => done(r.data.id, "saved"),
      onError: (e) => {
        if (isStale(e) && journal) refresh(journal.id)
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const mutation = journal ? update : create
  const errors = fieldErrors(mutation.error)
  const err = (field: string) => errors[`payload.${field}`]
  const setLine = (k: string, patch: Partial<Line>) =>
    setLines((ls) => ls.map((l) => (l.key === k ? { ...l, ...patch } : l)))
  const defaultCurrency = currencies.data?.data.find(
    (c) => c.default_currency
  )?.id

  return (
    <form
      className="flex flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault()
        const f = new FormData(e.currentTarget)
        const text = (k: string) => String(f.get(k) ?? "").trim()
        const payload: JournalPayload = {
          accounting_date: text("accounting_date"),
          currency_id: text("currency_id"),
          reason: text("reason"),
          evidence_ref: text("evidence_ref") || null,
          lines: lines.map((l) => ({
            account_id: l.account_id,
            debit: l.debit.trim() || "0",
            credit: l.credit.trim() || "0",
            source_ref: l.source_ref.trim() || null,
          })),
        }
        const client_ref = key.request.headers["Idempotency-Key"]
        if (journal)
          update.mutate({
            workspace: workspaceId,
            journal: journal.id,
            data: { client_ref, expected_version: journal.version, payload },
          })
        else
          create.mutate({
            workspace: workspaceId,
            data: { client_ref, payload },
          })
      }}
    >
      <Card className="gap-4 px-6 py-6 max-md:p-4">
        <div className="grid gap-4 md:grid-cols-2">
          <Field
            id="jn-date"
            label={t("accounting.journals.date")}
            errors={err("accounting_date")}
          >
            <Input
              type="date"
              name="accounting_date"
              required
              defaultValue={journal?.accounting_date ?? today()}
            />
          </Field>
          <Field
            id="jn-currency"
            label={t("accounting.journals.currency")}
            errors={err("currency_id")}
          >
            <CurrencyPicker
              name="currency_id"
              value={currency || defaultCurrency || ""}
              onChange={setCurrency}
              placeholder={t("accounting.journals.chooseCurrency")}
            />
          </Field>
          <Field
            id="jn-evidence"
            label={t("accounting.journals.evidence")}
            errors={err("evidence_ref")}
          >
            <Input
              name="evidence_ref"
              maxLength={255}
              defaultValue={journal?.evidence_ref ?? ""}
            />
          </Field>
        </div>
        <Field
          id="jn-reason"
          label={t("accounting.journals.reason")}
          errors={err("reason")}
        >
          <Textarea
            name="reason"
            required
            maxLength={1000}
            defaultValue={journal?.reason ?? ""}
          />
        </Field>
      </Card>

      <Card
        className="gap-4 px-6 py-6 max-md:p-4"
        role="region"
        aria-labelledby="jn-lines"
      >
        <h2
          id="jn-lines"
          className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
        >
          {t("accounting.journals.lines")}
        </h2>
        <p className="text-[13px] text-muted-foreground">
          {t("accounting.journals.linesHint")}
        </p>
        {err("lines")?.length ? (
          <p className="text-sm text-destructive">{err("lines")!.join(" ")}</p>
        ) : null}
        {lines.map((l, i) => (
          <fieldset
            key={l.key}
            className="grid gap-3 border-t border-(--hair) pt-4 first-of-type:border-0 first-of-type:pt-0 md:grid-cols-[2fr_1fr_1fr_1.5fr_auto] md:items-end"
          >
            <legend className="sr-only">
              {t("accounting.journals.line", { n: i + 1 })}
            </legend>
            <Field
              id={`jn-${l.key}-account`}
              label={t("accounting.journals.account")}
              errors={err(`lines.${i}.account_id`) ?? err(`lines.${i}`)}
            >
              <AccountPicker
                value={l.account_id}
                onChange={(id) => setLine(l.key, { account_id: id })}
                filter={(a) => a.active || a.id === l.account_id}
                placeholder={t("accounting.journals.chooseAccount")}
              />
            </Field>
            <Field
              id={`jn-${l.key}-debit`}
              label={t("accounting.journals.debit")}
              errors={err(`lines.${i}.debit`)}
            >
              <Input
                inputMode="decimal"
                value={l.debit}
                onChange={(e) => setLine(l.key, { debit: e.target.value })}
              />
            </Field>
            <Field
              id={`jn-${l.key}-credit`}
              label={t("accounting.journals.credit")}
              errors={err(`lines.${i}.credit`)}
            >
              <Input
                inputMode="decimal"
                value={l.credit}
                onChange={(e) => setLine(l.key, { credit: e.target.value })}
              />
            </Field>
            <Field
              id={`jn-${l.key}-ref`}
              label={t("accounting.journals.sourceRef")}
              errors={err(`lines.${i}.source_ref`)}
            >
              <Input
                maxLength={255}
                value={l.source_ref}
                onChange={(e) => setLine(l.key, { source_ref: e.target.value })}
              />
            </Field>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="max-md:justify-self-end"
              aria-label={t("accounting.journals.removeLine", { n: i + 1 })}
              disabled={lines.length <= 2}
              onClick={() =>
                setLines((ls) => ls.filter((x) => x.key !== l.key))
              }
            >
              <Trash2 className="size-4" />
            </Button>
          </fieldset>
        ))}
        <Button
          type="button"
          variant="outline"
          className="self-start"
          disabled={lines.length >= 100}
          onClick={() => setLines((ls) => [...ls, blankLine()])}
        >
          {t("accounting.journals.addLine")}
        </Button>
      </Card>

      <ApiError
        error={mutation.error}
        fallback={t("common.error")}
        codes={reasons}
      />
      <Button
        type="submit"
        size="lg"
        className="self-end max-md:w-full"
        disabled={mutation.isPending}
      >
        {t("accounting.journals.saveDraft")}
      </Button>
    </form>
  )
}
