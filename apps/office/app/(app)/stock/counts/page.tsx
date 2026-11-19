"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import {
  useStocktakesCreate,
  useStocktakesList,
} from "@verzo/api-client/endpoints/stocktakes"
import { useWarehousesList } from "@verzo/api-client/endpoints/warehouses"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { DialogClose, DialogFooter } from "@verzo/ui/components/dialog"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@verzo/ui/components/table"
import { Textarea } from "@verzo/ui/components/textarea"
import { formatDateTime } from "@verzo/ui/lib/format"
import { Field } from "@/components/field"
import { WarehousePicker } from "@/components/pickers"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"
import { useCan } from "@/lib/can"

import {
  Shell,
  StockPills,
  useAction,
  useCountReasons,
  useStateBadge,
} from "./shared"

/** Stock counts (Template List): one row per count, newest first; a new count in a dialog. */
export default function CountsPage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const badge = useStateBadge()
  const mayCreate = useCan("stocktake.create")
  const { page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const counts = useStocktakesList(
    workspaceId,
    { page: page || undefined },
    enabled
  )
  // shortcut: names for the first 100 warehouses; page these when a workspace holds more.
  const warehouses = useWarehousesList(
    workspaceId,
    { page_size: 100, roots_only: false },
    enabled
  )
  const warehouse = (id: string) =>
    warehouses.data?.data.find((w) => w.id === id)?.name ?? ""
  const columns = [
    { label: t("stocktakes.warehouse") },
    { label: t("stocktakes.reason") },
    { label: t("stocktakes.state") },
    { label: t("stocktakes.created"), right: true },
  ]
  const rows = counts.data?.data ?? []

  return (
    <>
      <PageHead
        eyebrow={t("shell.sell")}
        title={t("stocktakes.title")}
        actions={
          mayCreate ? (
            <NewCountDialog
              onCreated={(id) => router.push(`/stock/counts/${id}`)}
            />
          ) : null
        }
      />
      <ListCard
        filters={<StockPills view="counts" />}
        count={
          counts.data
            ? t("stocktakes.count", { total: counts.data.meta.total })
            : null
        }
        pager={
          counts.data?.data.length ? (
            <ListPager
              meta={counts.data.meta}
              onPage={(p) => router.push(`?page=${p}`)}
            />
          ) : null
        }
      >
        {counts.isPending ? (
          <ListSkeleton columns={columns} />
        ) : counts.isError ? (
          <CardState>
            <LoadError
              error={counts.error}
              onRetry={() => void counts.refetch()}
            />
          </CardState>
        ) : rows.length === 0 ? (
          <CardState>
            <Empty title={t("stocktakes.empty")} />
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
              {rows.map((s) => (
                <TableRow key={s.id}>
                  <TableCell>
                    <Link
                      className="font-semibold underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"
                      href={`/stock/counts/${s.id}`}
                    >
                      {warehouse(s.warehouse_id) || t("stocktakes.open")}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub" className="max-w-[40ch] truncate">
                    {s.reason}
                  </TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge status={s.state} {...badge(s.state)} />
                  </TableCell>
                  <TableCell
                    data-m="hide"
                    className="text-right text-muted-foreground"
                  >
                    {formatDateTime(s.created_at, locale)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
    </>
  )
}

/** A new count: warehouse and reason; the draft opens on its own page. */
function NewCountDialog({ onCreated }: { onCreated: (id: string) => void }) {
  const t = useTranslations()
  const reasons = useCountReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction({ done: () => {}, stale: () => {} })
  const create = useStocktakesCreate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: (r) => {
        a.setOpen(false)
        onCreated(r.data.id)
      },
    },
  })
  const errors = fieldErrors(create.error)
  const [warehouse, setWarehouse] = React.useState("")
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        create.reset()
      }}
      button={<Button />}
      trigger={t("stocktakes.new")}
      title={t("stocktakes.new")}
      description={t("stocktakes.newText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          create.mutate({
            workspace: workspaceId,
            data: {
              client_ref: a.clientRef,
              payload: {
                warehouse_id: String(f.get("warehouse_id") ?? ""),
                reason: String(f.get("reason") ?? "").trim(),
              },
            },
          })
        }}
      >
        <Field
          id="st-warehouse"
          label={t("stocktakes.warehouse")}
          errors={errors["payload.warehouse_id"]}
        >
          <WarehousePicker
            name="warehouse_id"
            value={warehouse}
            onChange={setWarehouse}
            placeholder={t("stocktakes.chooseWarehouse")}
          />
        </Field>
        <Field
          id="st-reason"
          label={t("stocktakes.reason")}
          errors={errors["payload.reason"]}
        >
          <Textarea name="reason" required maxLength={1000} />
        </Field>
        <ApiError
          error={create.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" disabled={create.isPending}>
            {t("stocktakes.create")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}
