"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getTillRegistersListQueryKey,
  useTillRegistersCreate,
  useTillRegistersList,
  useTillRegistersRotateKey,
} from "@genesy/api-client/endpoints/till"
import { useWarehousesList } from "@genesy/api-client/endpoints/warehouses"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Field, selectClass } from "@/components/field"
import { SecretReveal } from "@/components/secret-reveal"
import { StatusBadge } from "@/components/status-badge"
import { useTillMoney } from "@/lib/till-money"
import { Capped, Empty, LoadError } from "@/components/states"

export default function RegistersPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const [page, setPage] = React.useState(1)
  const registers = useTillRegistersList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  // shortcut: first 100 warehouses for names and the picker; page when a workspace has more.
  const warehouses = useWarehousesList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getTillRegistersListQueryKey(workspaceId),
    })
  const [rotated, setRotated] = React.useState<{
    name: string
    key: string
  } | null>(null)
  const rotate = useTillRegistersRotateKey({ mutation: { onSuccess: refresh } })
  const warehouseName = (id: string) =>
    warehouses.data?.data.find((w) => w.id === id)?.name ?? ""

  return (
    <>
      <div className="flex justify-end">
        <CreateRegister
          warehouses={warehouses.data?.data ?? []}
          warehousesMeta={warehouses.data?.meta}
          onCreated={refresh}
        />
      </div>
      <ApiError error={rotate.error} fallback={t("common.error")} />
      <Dialog
        open={rotated !== null}
        onOpenChange={(o) => !o && setRotated(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {t("till.newKeyFor", { name: rotated?.name ?? "" })}
            </DialogTitle>
          </DialogHeader>
          {rotated ? <SecretReveal secret={rotated.key} /> : null}
        </DialogContent>
      </Dialog>
      {registers.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : registers.isError ? (
        <LoadError
          error={registers.error}
          onRetry={() => void registers.refetch()}
        />
      ) : registers.data.data.length === 0 ? (
        <Empty title={t("till.noRegisters")} />
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("appKeys.name")}</TableHead>
                <TableHead>{t("stock.warehouse")}</TableHead>
                <TableHead>{t("till.deviceKey")}</TableHead>
                <TableHead>
                  <span className="sr-only">{t("appKeys.actions")}</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {registers.data.data.map((r) => (
                <TableRow key={r.id}>
                  <TableCell className="font-medium">{r.name}</TableCell>
                  <TableCell>{warehouseName(r.warehouse_id)}</TableCell>
                  <TableCell>
                    {r.key_prefix ? (
                      <span className="font-mono text-xs">{r.key_prefix}…</span>
                    ) : (
                      <StatusBadge
                        status="revoked"
                        label={t("appKeys.revoked")}
                      />
                    )}
                  </TableCell>
                  <TableCell className="text-right">
                    <ConfirmDialog
                      trigger={t("till.rotate")}
                      title={t("till.rotateTitle", { name: r.name })}
                      description={t("till.rotateText")}
                      confirm={t("till.rotate")}
                      cancel={t("actions.cancel")}
                      disabled={rotate.isPending}
                      onConfirm={() =>
                        rotate.mutate(
                          { workspace: workspaceId, register: r.id },
                          {
                            onSuccess: ({ data }) =>
                              setRotated({ name: r.name, key: data.key }),
                          }
                        )
                      }
                    />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pager
            page={registers.data.meta.current_page}
            lastPage={registers.data.meta.last_page}
            summary={t("pager.summary", {
              from: registers.data.meta.from ?? 0,
              to: registers.data.meta.to ?? 0,
              total: registers.data.meta.total,
            })}
            previous={t("pager.previous")}
            next={t("pager.next")}
            onPage={setPage}
          />
        </>
      )}
    </>
  )
}

function CreateRegister({
  warehouses,
  warehousesMeta,
  onCreated,
}: {
  warehouses: { id: string; name: string }[]
  warehousesMeta?: { total: number; per_page: number }
  onCreated: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const { currencies } = useTillMoney()
  const [open, setOpen] = React.useState(false)
  const create = useTillRegistersCreate({ mutation: { onSuccess: onCreated } })
  const errors = fieldErrors(create.error)
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        // The device key is shown once; forget it when the dialog closes.
        if (!o) create.reset()
      }}
    >
      <DialogTrigger render={<Button />}>{t("till.newRegister")}</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("till.newRegister")}</DialogTitle>
        </DialogHeader>
        {create.data ? (
          <SecretReveal secret={create.data.data.key} />
        ) : (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              const f = new FormData(e.currentTarget)
              create.mutate({
                workspace: workspaceId,
                data: {
                  name: String(f.get("name") ?? ""),
                  warehouse_id: String(f.get("warehouse_id") ?? ""),
                  currency_id: String(f.get("currency_id") ?? "") || null,
                },
              })
            }}
          >
            <Field id="reg-name" label={t("appKeys.name")} errors={errors.name}>
              <Input name="name" required />
            </Field>
            <Capped meta={warehousesMeta} />
            <Field
              id="reg-warehouse"
              label={t("till.storeWarehouse")}
              errors={errors.warehouse_id}
            >
              <select name="warehouse_id" className={selectClass} required>
                {warehouses.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field
              id="reg-currency"
              label={t("till.currency")}
              errors={errors.currency_id}
            >
              <select name="currency_id" className={selectClass}>
                <option value="">{t("till.mainCurrency")}</option>
                {currencies.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.asset_type}
                  </option>
                ))}
              </select>
            </Field>
            <ApiError error={create.error} fallback={t("common.error")} />
            <DialogFooter>
              <Button type="submit" disabled={create.isPending}>
                {t("actions.save")}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
