"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getTillRegistersListQueryKey,
  useTillRegistersCreate,
  useTillRegistersList,
  useTillRegistersPairing,
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
import { StatusBadge } from "@/components/status-badge"
import { useTillMoney } from "@/lib/till-money"
import { Capped, Done, Empty, LoadError, useDone } from "@/components/states"

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
  const done = useDone()
  const [pairing, setPairing] = React.useState<{ id: string; name: string } | null>(
    null
  )
  // Revoking = a new key nobody sees: the till stops and must be paired again.
  const revoke = useTillRegistersRotateKey({ mutation: { onSuccess: refresh } })
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
      <ApiError error={revoke.error} fallback={t("common.error")} />
      <Done message={done.message} />
      <Dialog
        open={pairing !== null}
        onOpenChange={(o) => {
          if (!o) {
            setPairing(null)
            refresh()
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {t("till.pairTitle", { name: pairing?.name ?? "" })}
            </DialogTitle>
          </DialogHeader>
          {pairing ? <PairCode register={pairing.id} /> : null}
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
                <TableHead>{t("till.paired")}</TableHead>
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
                    {r.paired_at ? (
                      new Date(r.paired_at).toLocaleString()
                    ) : (
                      <StatusBadge status="draft" label={t("till.notPaired")} />
                    )}
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex flex-wrap justify-end gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setPairing({ id: r.id, name: r.name })}
                      >
                        {t("till.pair")}
                      </Button>
                      <ConfirmDialog
                        trigger={t("till.revoke")}
                        title={t("till.revokeTitle", { name: r.name })}
                        description={t("till.revokeText")}
                        confirm={t("till.revoke")}
                        cancel={t("actions.cancel")}
                        disabled={revoke.isPending}
                        onConfirm={() =>
                          revoke.mutate(
                            { workspace: workspaceId, register: r.id },
                            {
                              onSuccess: () =>
                                done.show(t("till.revoked", { name: r.name })),
                            }
                          )
                        }
                      />
                    </div>
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
        if (!o) create.reset()
      }}
    >
      <DialogTrigger render={<Button />}>{t("till.newRegister")}</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("till.newRegister")}</DialogTitle>
        </DialogHeader>
        {create.data ? (
          // The new register's key is never shown: the till pairs with a code.
          <PairCode register={create.data.data.id} />
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

/** A fresh one-time pairing code for `register`: the shop name and code to type on the till. */
function PairCode({ register }: { register: string }) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const issue = useTillRegistersPairing()
  const { mutate } = issue
  React.useEffect(() => {
    mutate({ workspace: workspaceId, register })
  }, [mutate, workspaceId, register])
  const code = issue.data?.data
  return (
    <div className="flex flex-col gap-4">
      <ApiError error={issue.error} fallback={t("common.error")} />
      {code ? (
        <>
          <p className="text-sm text-muted-foreground">{t("till.pairHint")}</p>
          <dl className="grid grid-cols-[auto_1fr] items-baseline gap-x-4 gap-y-2">
            <dt className="text-sm text-muted-foreground">{t("till.shopName")}</dt>
            <dd className="font-mono text-lg">{code.workspace}</dd>
            <dt className="text-sm text-muted-foreground">{t("till.pairCode")}</dt>
            <dd
              aria-label={t("till.pairCode")}
              className="font-mono text-3xl font-semibold tracking-widest"
            >
              {code.code}
            </dd>
          </dl>
          <p className="text-sm text-muted-foreground">
            {t("till.pairValid", {
              time: new Date(code.expires_at).toLocaleTimeString([], {
                hour: "2-digit",
                minute: "2-digit",
              }),
            })}
          </p>
        </>
      ) : issue.isPending ? (
        <p role="status" className="text-sm text-muted-foreground">
          {t("common.loading")}
        </p>
      ) : null}
      <DialogFooter>
        <Button
          variant="outline"
          disabled={issue.isPending}
          onClick={() => mutate({ workspace: workspaceId, register })}
        >
          {t("till.newCode")}
        </Button>
      </DialogFooter>
    </div>
  )
}
