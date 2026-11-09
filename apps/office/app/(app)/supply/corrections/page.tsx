"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getFarmSupplyCorrectionListQueryKey,
  useFarmSupplyCorrectionApprove,
  useFarmSupplyCorrectionList,
  useFarmSupplyCorrectionReject,
  useFarmSupplyCorrectionRequest,
  useFarmSupplyList,
} from "@genesy/api-client/endpoints/farm-supply"
import { useUsersMe } from "@genesy/api-client/endpoints/users"
import type { Money, SupplyCorrectionResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatMoney } from "@genesy/ui/lib/money"
import { Field, selectClass } from "@/components/field"
import { CardState, ListCard, ListPager, ListSkeleton } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"

function useMoney() {
  const locale = useLocale()
  return (m: Money) => formatMoney(m.minor, m.decimals, m.currency, locale)
}

/** Kg corrections of bought deliveries: filed by one ops/admin, decided by another. */
export default function SupplyCorrectionsPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const me = useUsersMe()
  const myId = me.data?.data.user.id
  const [page, setPage] = React.useState(1)
  const list = useFarmSupplyCorrectionList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const columns = [
    { label: t("corrections.farmers") },
    { label: t("corrections.reason") },
    { label: t("corrections.netKg"), right: true },
    { label: t("lots.status") },
  ]

  return (
    <ListCard
      count={
        list.data
          ? t("corrections.count", { total: list.data.meta.total })
          : null
      }
      filters={<FileDialog />}
      pager={
        list.data?.data.length ? (
          <ListPager meta={list.data.meta} onPage={setPage} />
        ) : null
      }
    >
      {list.isPending ? (
        <ListSkeleton columns={columns} />
      ) : list.isError ? (
        <CardState>
          <LoadError error={list.error} onRetry={() => void list.refetch()} />
        </CardState>
      ) : list.data.data.length === 0 ? (
        <CardState>
          <Empty title={t("corrections.none")} />
        </CardState>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              {columns.map((c) => (
                <TableHead
                  key={c.label}
                  className={c.right ? "text-right" : undefined}
                >
                  {c.label}
                </TableHead>
              ))}
              <TableHead>
                <span className="sr-only">{t("appKeys.actions")}</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {list.data.data.map((c) => (
              <TableRow key={c.id}>
                <TableCell className="font-semibold">
                  {c.lines.map((l) => (
                    <div key={`${l.share_id}-${l.person_id}`}>
                      {l.kg_before != null
                        ? t("corrections.lineKg", {
                            name: l.farmer_name,
                            before: l.kg_before,
                            after: l.kg_after ?? "",
                          })
                        : t("corrections.lineDelta", {
                            name: l.farmer_name,
                            delta: l.kg_delta,
                          })}
                    </div>
                  ))}
                </TableCell>
                <TableCell data-m="sub" className="text-muted-foreground">
                  {c.reason}
                </TableCell>
                <TableCell data-m="right" className="text-right tabular-nums">
                  {t("corrections.kg", { kg: c.net_kg })}
                </TableCell>
                <TableCell data-m="badge">
                  <StatusBadge
                    status={
                      c.status === "approved"
                        ? "active"
                        : c.status === "rejected"
                          ? "cancelled"
                          : "pending"
                    }
                    label={t(`corrections.statuses.${c.status}`)}
                  />
                </TableCell>
                <TableCell data-m="sub" className="text-right">
                  {c.status !== "pending" ? null : c.requested_by === myId ? (
                    <span className="text-xs text-muted-foreground">
                      {t("corrections.waitingOther")}
                    </span>
                  ) : (
                    <DecideDialog correction={c} />
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </ListCard>
  )
}

/** Second person reviews the before/after kg, stock change and balances, then decides. */
function DecideDialog({
  correction,
}: {
  correction: SupplyCorrectionResource
}) {
  const t = useTranslations()
  const money = useMoney()
  const queryClient = useQueryClient()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const done = {
    mutation: {
      onSuccess: () => {
        void queryClient.invalidateQueries({
          queryKey: getFarmSupplyCorrectionListQueryKey(workspaceId),
        })
        setOpen(false)
      },
    },
  }
  const approve = useFarmSupplyCorrectionApprove(done)
  const reject = useFarmSupplyCorrectionReject(done)
  const vars = { workspace: workspaceId, correction: correction.id }
  const owing = correction.farmers.filter(
    (f) => f.balance_after && f.balance_after.minor < 0
  )

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) {
          approve.reset()
          reject.reset()
        }
      }}
    >
      <DialogTrigger render={<Button />}>
        {t("corrections.review")}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("corrections.reviewTitle")}</DialogTitle>
          <DialogDescription>{correction.reason}</DialogDescription>
        </DialogHeader>
        <ul className="flex flex-col gap-1 text-sm">
          {correction.lines.map((l) => (
            <li key={`${l.share_id}-${l.person_id}`}>
              {t("corrections.lineKg", {
                name: l.farmer_name,
                before: l.kg_before ?? "",
                after: l.kg_after ?? "",
              })}
            </li>
          ))}
          <li className="text-muted-foreground">
            {t("corrections.stock", { kg: correction.net_kg })}
          </li>
        </ul>
        {correction.farmers.length ? (
          <ul className="flex flex-col gap-1 text-sm">
            {correction.farmers.map((f) => (
              <li key={f.person_id}>
                {t("corrections.money", {
                  name: f.name,
                  change: money(f.change),
                  after: f.balance_after ? money(f.balance_after) : "—",
                })}
              </li>
            ))}
          </ul>
        ) : null}
        {owing.map((f) => (
          <Notice
            key={f.person_id}
            variant="warning"
            title={t("corrections.owesTitle")}
          >
            {t("corrections.owes", {
              name: f.name,
              amount: money({
                ...f.balance_after!,
                minor: -f.balance_after!.minor,
              }),
            })}
          </Notice>
        ))}
        <ApiError
          error={approve.error ?? reject.error}
          fallback={t("common.error")}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button
            type="button"
            variant="outline"
            disabled={approve.isPending || reject.isPending}
            onClick={() => reject.mutate(vars)}
          >
            {t("corrections.reject")}
          </Button>
          <Button
            type="button"
            disabled={approve.isPending || reject.isPending}
            onClick={() => approve.mutate(vars)}
          >
            {t("corrections.approve")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/** File a correction: a batch line, the right kg, and optionally the right farmer. */
function FileDialog() {
  const t = useTranslations()
  const queryClient = useQueryClient()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const [batchId, setBatchId] = React.useState("")
  // shortcut: newest 100 batches in the picker; add search when a farm buys more.
  const batches = useFarmSupplyList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: open && Boolean(workspaceId) } }
  )
  const batch = batches.data?.data.find((b) => b.id === batchId)
  const request = useFarmSupplyCorrectionRequest({
    mutation: {
      onSuccess: () => {
        void queryClient.invalidateQueries({
          queryKey: getFarmSupplyCorrectionListQueryKey(workspaceId),
        })
        setOpen(false)
      },
    },
  })
  const errors = fieldErrors(request.error)

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) request.reset()
      }}
    >
      <DialogTrigger render={<Button />}>{t("corrections.file")}</DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const share = String(f.get("share_id") ?? "")
            const to = String(f.get("person_id") ?? "")
            const kg = (name: string) => String(f.get(name) ?? "").trim()
            // Wrong farmer: two lines on the share, each with the kg that farmer should have.
            const lines = [
              { share_id: share, kg: kg("kg") },
              ...(to
                ? [{ share_id: share, person_id: to, kg: kg("to_kg") }]
                : []),
            ]
            request.mutate({
              workspace: workspaceId,
              batch: batchId,
              data: {
                client_ref: crypto.randomUUID(),
                reason: String(f.get("reason") ?? "").trim(),
                lines,
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("corrections.file")}</DialogTitle>
            <DialogDescription>{t("corrections.fileText")}</DialogDescription>
          </DialogHeader>
          {batches.isError ? (
            <LoadError
              error={batches.error}
              onRetry={() => void batches.refetch()}
            />
          ) : null}
          <Field id="cor-batch" label={t("corrections.batch")}>
            <select
              className={selectClass}
              value={batchId}
              onChange={(e) => setBatchId(e.target.value)}
              required
            >
              <option value="" disabled>
                {t("corrections.pickBatch")}
              </option>
              {batches.data?.data.map((b) => (
                <option key={b.id} value={b.id}>
                  {b.lot.code}
                </option>
              ))}
            </select>
          </Field>
          <Field
            id="cor-share"
            label={t("corrections.delivery")}
            errors={errors["lines.0.share_id"] ?? errors.lines ?? errors.batch}
          >
            <select name="share_id" className={selectClass} required>
              {batch?.lines.map((l) => (
                <option key={l.share_id} value={l.share_id}>
                  {t("corrections.deliveryOption", {
                    name: l.farmer_name,
                    kg: l.kg,
                  })}
                </option>
              ))}
            </select>
          </Field>
          <Field
            id="cor-kg"
            label={t("corrections.rightKg")}
            errors={errors["lines.0.kg"]}
          >
            <Input name="kg" inputMode="decimal" required />
          </Field>
          <Field
            id="cor-person"
            label={t("corrections.rightFarmer")}
            errors={errors["lines.1.person_id"]}
          >
            {/* shortcut: farmers of this batch only; the API also takes any farmer of the village. */}
            <select name="person_id" className={selectClass} defaultValue="">
              <option value="">{t("corrections.sameFarmer")}</option>
              {[
                ...new Map(
                  batch?.lines.map((l) => [l.person_id, l.farmer_name])
                ).entries(),
              ].map(([id, name]) => (
                <option key={id} value={id}>
                  {name}
                </option>
              ))}
            </select>
          </Field>
          <Field
            id="cor-to-kg"
            label={t("corrections.rightFarmerKg")}
            errors={errors["lines.1.kg"]}
          >
            <Input name="to_kg" inputMode="decimal" />
          </Field>
          <Field
            id="cor-reason"
            label={t("corrections.reason")}
            errors={errors.reason}
          >
            <Input name="reason" maxLength={500} required />
          </Field>
          <ApiError error={request.error} fallback={t("common.error")} />
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              {t("actions.cancel")}
            </DialogClose>
            <Button type="submit" disabled={request.isPending || !batchId}>
              {t("corrections.send")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
