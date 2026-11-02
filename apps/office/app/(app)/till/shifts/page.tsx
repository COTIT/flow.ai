"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useTillShiftsList } from "@genesy/api-client/endpoints/till"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { CardState, ListCard, ListPager, ListSkeleton } from "@/components/list"
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
  const columns = [
    { label: t("till.register") },
    { label: t("till.openedAt") },
    { label: t("till.closedAt") },
    { label: t("till.opening"), right: true },
    { label: t("till.expected"), right: true },
    { label: t("till.counted"), right: true },
    { label: t("till.difference"), right: true },
  ]

  return (
    <ListCard
      count={
        shifts.data
          ? t("till.shiftsCount", { total: shifts.data.meta.total })
          : null
      }
      pager={
        shifts.data?.data.length ? (
          <ListPager meta={shifts.data.meta} onPage={setPage} />
        ) : null
      }
    >
      {shifts.isPending ? (
        <ListSkeleton columns={columns} />
      ) : shifts.isError ? (
        <CardState>
          <LoadError
            error={shifts.error}
            onRetry={() => void shifts.refetch()}
          />
        </CardState>
      ) : shifts.data.data.length === 0 ? (
        <CardState>
          <Empty title={t("till.noShifts")} />
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
            </TableRow>
          </TableHeader>
          <TableBody>
            {shifts.data.data.map((s) => (
              <TableRow key={s.id}>
                <TableCell className="font-semibold">
                  {register(s.register_id)?.name}
                </TableCell>
                <TableCell data-m="sub" className="tabular-nums">
                  {when(s.opened_at)}
                </TableCell>
                <TableCell data-m="sub" className="tabular-nums">
                  {s.closed_at ? (
                    when(s.closed_at)
                  ) : (
                    <StatusBadge status="active" label={t("till.openShift")} />
                  )}
                </TableCell>
                <TableCell data-m="hide" className="text-right tabular-nums">
                  {registerMoney(s.opening, s.register_id)}
                </TableCell>
                <TableCell data-m="hide" className="text-right tabular-nums">
                  {registerMoney(s.expected, s.register_id)}
                </TableCell>
                <TableCell
                  data-m="right"
                  className="text-right font-semibold tabular-nums"
                >
                  {registerMoney(s.counted, s.register_id)}
                </TableCell>
                {/* The API's own counted − expected: zero = balanced (teal), else short/over (gold). */}
                <TableCell data-m="badge" className="text-right tabular-nums">
                  {s.difference == null ? (
                    "—"
                  ) : (
                    <StatusBadge
                      status=""
                      tone={s.difference === 0 ? "teal" : "gold"}
                      label={registerMoney(s.difference, s.register_id)}
                    />
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
