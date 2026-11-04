"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useTillShiftsList } from "@genesy/api-client/endpoints/till"
import { Switch } from "@genesy/ui/components/switch"
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

import { RegisterSelect } from "../register-select"

export default function ShiftsPage({
  searchParams,
}: {
  searchParams: Promise<{ open?: string; register_id?: string; page?: string }>
}) {
  const t = useTranslations()
  const router = useRouter()
  const { open, register_id, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const { register, registerMoney } = useTillMoney()
  const shifts = useTillShiftsList(
    workspaceId,
    {
      open: open === "true" || undefined,
      register_id: register_id || undefined,
      page: Number(page) || undefined,
    },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const go = (next: { open?: boolean; register?: string; page?: number }) => {
    const params = new URLSearchParams()
    if (next.open ?? open === "true") params.set("open", "true")
    const reg = next.register ?? register_id
    if (reg) params.set("register_id", reg)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }
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
      filters={
        <>
          <label className="flex min-h-11 items-center gap-3 text-sm">
            <Switch
              // Uncontrolled so it flips at once; the key resyncs it when the URL changes.
              key={open ?? ""}
              defaultChecked={open === "true"}
              onCheckedChange={(checked) => go({ open: checked, page: 1 })}
            />
            {t("till.onlyOpen")}
          </label>
          <RegisterSelect
            value={register_id ?? ""}
            onChange={(id) => go({ register: id, page: 1 })}
          />
        </>
      }
      count={
        shifts.data
          ? t("till.shiftsCount", { total: shifts.data.meta.total })
          : null
      }
      pager={
        shifts.data?.data.length ? (
          <ListPager
            meta={shifts.data.meta}
            onPage={(p) => go({ page: p })}
          />
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
          <Empty
            title={t("till.noShifts")}
            onClear={
              open === "true" || register_id ? () => router.push("?") : undefined
            }
          />
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
