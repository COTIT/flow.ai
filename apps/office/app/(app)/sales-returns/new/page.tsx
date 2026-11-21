"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@verzo/api-client/auth"
import {
  getSalesReturnsListQueryKey,
  useSalesReturnsCandidates,
  useSalesReturnsCreate,
} from "@verzo/api-client/endpoints/sales-returns"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { Card } from "@verzo/ui/components/card"
import { Input } from "@verzo/ui/components/input"
import { Notice } from "@verzo/ui/components/notice"
import { formatQty } from "@verzo/ui/lib/format"
import { DetailSkeleton } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { PageHead } from "@/components/list"
import { LoadError, NoAccess } from "@/components/states"
import { useCan, useRights } from "@/lib/can"
import { useIdempotencyKey } from "@/lib/idempotency"

import { useBookToday, useReturnReasons, useWord } from "../shared"

/**
 * Credit goods of one order's recognised sale: per sale line, what the server says is still
 * left to credit, a quantity per line. Creates a draft; posting reviews the credit amounts.
 */
export default function NewSalesReturnPage({
  searchParams,
}: {
  searchParams: Promise<{ order?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const word = useWord()
  const reasons = useReturnReasons()
  const queryClient = useQueryClient()
  const rights = useRights()
  const may = useCan("salesreturn.create")
  const { order } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const candidates = useSalesReturnsCandidates(
    workspaceId,
    { order_id: order ?? "" },
    { query: { enabled: Boolean(workspaceId && order) && may } }
  )
  const key = useIdempotencyKey()
  const [sale, setSale] = React.useState("")
  const [qty, setQty] = React.useState<Record<string, string>>({})
  const [reason, setReason] = React.useState("")
  const bookToday = useBookToday()
  const [picked, setDate] = React.useState("")
  const date = picked || bookToday
  const create = useSalesReturnsCreate({
    mutation: {
      onSuccess: (r) => {
        void queryClient.invalidateQueries({ queryKey: getSalesReturnsListQueryKey(workspaceId) })
        router.push(`/sales-returns/${r.data.id}?done=created`)
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const errors = fieldErrors(create.error)
  const err = (k: string) => errors[`payload.${k}`]
  const head = (
    <PageHead
      back={{ href: "/sales-returns", label: t("salesReturns.title") }}
      eyebrow={t("shell.sell")}
      title={t("salesReturns.new.title")}
    />
  )
  if (rights.read && !may) return <NoAccess />
  if (!order)
    return (
      <>
        {head}
        <Notice variant="info" title={t("salesReturns.new.fromOrder")} />
      </>
    )
  if (candidates.isPending) return <DetailSkeleton />
  if (candidates.isError)
    return <LoadError error={candidates.error} onRetry={() => void candidates.refetch()} backHref="/sales-returns" />
  const c = candidates.data.data
  const open = c.recognitions.filter((r) => r.lines.some((l) => Number(l.creditable) > 0))
  const current = open.find((r) => r.recognition_id === sale) ?? open[0]
  const lines = (current?.lines ?? [])
    .filter((l) => qty[l.recognition_line_id]?.trim())
    .map((l) => ({ recognition_line_id: l.recognition_line_id, quantity: qty[l.recognition_line_id]!.trim() }))

  return (
    <>
      {head}
      <form
        className="flex flex-col gap-5"
        onSubmit={(e) => {
          e.preventDefault()
          if (!current) return
          create.mutate({
            workspace: workspaceId,
            data: {
              client_ref: key.request.headers["Idempotency-Key"],
              payload: {
                recognition_id: current.recognition_id,
                accounting_date: date,
                reason: reason.trim(),
                lines,
              },
            },
          })
        }}
      >
        <Card className="gap-4 px-6 py-6 max-md:p-4">
          <p className="text-[13px]">
            {t("salesReturns.new.ofOrder", {
              number: c.order_number,
              customer: c.relation_name ?? t("salesReturns.anonymous"),
            })}
          </p>
          {current ? (
            <div className="grid gap-4 md:grid-cols-2">
              {open.length > 1 ? (
                <Field id="sr-sale" label={t("salesReturns.new.sale")} errors={err("recognition_id")}>
                  <select
                    className={selectClass}
                    value={current.recognition_id}
                    onChange={(e) => {
                      setSale(e.target.value)
                      setQty({})
                    }}
                  >
                    {open.map((r) => (
                      <option key={r.recognition_id} value={r.recognition_id}>
                        {t("salesReturns.new.saleOption", {
                          date: r.accounting_date,
                          kind: word("kinds", r.kind),
                        })}
                      </option>
                    ))}
                  </select>
                </Field>
              ) : null}
              <Field id="sr-date" label={t("salesReturns.date")} errors={err("accounting_date")}>
                <Input type="date" required value={date} onChange={(e) => setDate(e.target.value)} />
              </Field>
              <Field id="sr-reason" label={t("salesReturns.reason")} errors={err("reason")}>
                <Input required maxLength={1000} value={reason} onChange={(e) => setReason(e.target.value)} />
              </Field>
            </div>
          ) : null}
        </Card>

        <Card className="gap-4 px-6 py-6 max-md:p-4" role="region" aria-labelledby="sr-lines">
          <h2 id="sr-lines" className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase">
            {t("salesReturns.lines")}
          </h2>
          {err("lines")?.length ? <p className="text-sm text-destructive">{err("lines")!.join(" ")}</p> : null}
          {current ? null : <Notice title={t("salesReturns.new.nothingToCredit")} />}
          {current?.lines.map((l) => {
            const i = lines.findIndex((x) => x.recognition_line_id === l.recognition_line_id)
            const le = (f: string) => (i >= 0 ? err(`lines.${i}.${f}`) : undefined)
            const none = Number(l.creditable) === 0
            return (
              <Field
                key={l.recognition_line_id}
                id={`srl-${l.recognition_line_id}`}
                label={`${l.product_name ?? t("salesReturns.item")} · ${t("salesReturns.new.sold", {
                  qty: formatQty(l.recognised, locale),
                })} · ${t("salesReturns.new.left", { qty: formatQty(l.creditable, locale) })}`}
                errors={le("quantity") ?? le("recognition_line_id")}
              >
                <Input
                  inputMode="decimal"
                  disabled={none}
                  value={qty[l.recognition_line_id] ?? ""}
                  onChange={(e) => setQty((q) => ({ ...q, [l.recognition_line_id]: e.target.value }))}
                />
              </Field>
            )
          })}
        </Card>

        <ApiError error={create.error} fallback={t("common.error")} codes={reasons} />
        <Button
          type="submit"
          size="lg"
          className="self-end max-md:w-full"
          disabled={create.isPending || !current || lines.length === 0 || !reason.trim()}
        >
          {t("salesReturns.new.saveDraft")}
        </Button>
      </form>
    </>
  )
}
