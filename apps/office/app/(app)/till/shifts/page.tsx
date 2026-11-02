"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useTillShiftsList } from "@genesy/api-client/endpoints/till"
import { ApiError } from "@genesy/ui/components/api-error"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { StatusBadge } from "@/components/status-badge"
import { useTillMoney } from "@/lib/till-money"
import { Empty, LoadError } from "@/components/states"

export default function ShiftsPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [page, setPage] = React.useState(1)
  const { register, registerMoney } = useTillMoney()
  const shifts = useTillShiftsList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const when = (s: string | null) => (s ? new Date(s).toLocaleString() : "—")

  if (shifts.isPending)
    return (
      <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
    )
  if (shifts.isError)
    return (
      <LoadError error={shifts.error} onRetry={() => void shifts.refetch()} />
    )
  if (shifts.data.data.length === 0) return <Empty title={t("till.noShifts")} />
  return (
    <>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("till.register")}</TableHead>
            <TableHead>{t("till.openedAt")}</TableHead>
            <TableHead>{t("till.closedAt")}</TableHead>
            <TableHead className="text-right">{t("till.opening")}</TableHead>
            <TableHead className="text-right">{t("till.expected")}</TableHead>
            <TableHead className="text-right">{t("till.counted")}</TableHead>
            <TableHead className="text-right">{t("till.difference")}</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {shifts.data.data.map((s) => (
            <TableRow key={s.id}>
              <TableCell>{register(s.register_id)?.name}</TableCell>
              <TableCell className="tabular-nums">
                {when(s.opened_at)}
              </TableCell>
              <TableCell className="tabular-nums">
                {s.closed_at ? (
                  when(s.closed_at)
                ) : (
                  <StatusBadge status="active" label={t("till.openShift")} />
                )}
              </TableCell>
              <TableCell className="text-right tabular-nums">
                {registerMoney(s.opening, s.register_id)}
              </TableCell>
              <TableCell className="text-right tabular-nums">
                {registerMoney(s.expected, s.register_id)}
              </TableCell>
              <TableCell className="text-right tabular-nums">
                {registerMoney(s.counted, s.register_id)}
              </TableCell>
              {/* The API's own counted − expected; negative = short. */}
              <TableCell
                className={`text-right tabular-nums ${s.difference && s.difference < 0 ? "text-destructive" : ""}`}
              >
                {registerMoney(s.difference, s.register_id)}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      <Pager
        page={shifts.data.meta.current_page}
        lastPage={shifts.data.meta.last_page}
        summary={t("pager.summary", {
          from: shifts.data.meta.from ?? 0,
          to: shifts.data.meta.to ?? 0,
          total: shifts.data.meta.total,
        })}
        previous={t("pager.previous")}
        next={t("pager.next")}
        onPage={setPage}
      />
    </>
  )
}
