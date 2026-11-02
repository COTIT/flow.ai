"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import { useProductsList } from "@genesy/api-client/endpoints/products"
import {
  getSupplyChainListQueryKey,
  useSupplyChainList,
  useSupplyChainReceive,
} from "@genesy/api-client/endpoints/supply-chain"
import { useWarehousesList } from "@genesy/api-client/endpoints/warehouses"
import type { SupplyTransferResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
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
import { Empty, LoadError } from "@/components/states"

export default function SupplyTransfersPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [page, setPage] = React.useState(1)
  const transfers = useSupplyChainList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const label = (group: string, key: string) =>
    t.has(`supply.${group}.${key}`) ? t(`supply.${group}.${key}`) : key

  if (transfers.isPending)
    return (
      <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
    )
  if (transfers.isError)
    return (
      <LoadError
        error={transfers.error}
        onRetry={() => void transfers.refetch()}
      />
    )
  if (transfers.data.data.length === 0)
    return <Empty title={t("supply.noTransfers")} />
  return (
    <>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("supply.partner")}</TableHead>
            <TableHead>{t("supply.direction")}</TableHead>
            <TableHead>{t("supply.fromLot")}</TableHead>
            <TableHead className="text-right">{t("supply.quantity")}</TableHead>
            <TableHead>{t("lots.status")}</TableHead>
            <TableHead>{t("supply.received")}</TableHead>
            <TableHead>
              <span className="sr-only">{t("appKeys.actions")}</span>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {transfers.data.data.map((tr) => (
            <TableRow key={tr.id}>
              <TableCell className="font-medium">{tr.partner.name}</TableCell>
              <TableCell>{label("transferDirections", tr.direction)}</TableCell>
              <TableCell>
                {tr.from_lot.code}{" "}
                <span className="font-mono text-xs text-muted-foreground">
                  {tr.from_lot.public_code}
                </span>
              </TableCell>
              <TableCell className="text-right tabular-nums">
                {tr.quantity}
              </TableCell>
              <TableCell>
                <StatusBadge
                  status={tr.status === "received" ? "active" : tr.status}
                  label={label("transferStatuses", tr.status)}
                />
              </TableCell>
              <TableCell>
                {tr.to_lot ? (
                  <Link
                    className="underline-offset-4 hover:underline"
                    href={`/lots/${tr.to_lot.id}`}
                  >
                    {t("supply.receivedInto", {
                      quantity: tr.received_quantity ?? "",
                      code: tr.to_lot.code,
                    })}
                  </Link>
                ) : (
                  "—"
                )}
              </TableCell>
              <TableCell className="text-right">
                {tr.direction === "incoming" && tr.status === "dispatched" ? (
                  <ReceiveDialog transfer={tr} />
                ) : null}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      <Pager
        page={transfers.data.meta.current_page}
        lastPage={transfers.data.meta.last_page}
        summary={t("pager.summary", {
          from: transfers.data.meta.from ?? 0,
          to: transfers.data.meta.to ?? 0,
          total: transfers.data.meta.total,
        })}
        previous={t("pager.previous")}
        next={t("pager.next")}
        onPage={setPage}
      />
    </>
  )
}

/** Take a dispatched transfer into a new lot here; once only, so the dialog says so. */
function ReceiveDialog({ transfer }: { transfer: SupplyTransferResource }) {
  const t = useTranslations()
  const router = useRouter()
  const queryClient = useQueryClient()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const enabled = { query: { enabled: open && Boolean(workspaceId) } }
  // shortcut: first 100 products and warehouses in the pickers; add search when catalogs grow.
  const products = useProductsList(workspaceId, { page_size: 100 }, enabled)
  const warehouses = useWarehousesList(workspaceId, { page_size: 100 }, enabled)
  const receive = useSupplyChainReceive({
    mutation: {
      onSuccess: ({ data }) => {
        void queryClient.invalidateQueries({
          queryKey: getSupplyChainListQueryKey(workspaceId),
        })
        setOpen(false)
        if (data.to_lot) router.push(`/lots/${data.to_lot.id}?done=received`)
      },
    },
  })
  const errors = fieldErrors(receive.error)

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) receive.reset()
      }}
    >
      <DialogTrigger
        render={<Button />}
        aria-label={t("supply.receiveFrom", { code: transfer.from_lot.code })}
      >
        {t("supply.receive")}
      </DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            receive.mutate({
              workspace: workspaceId,
              transfer: transfer.id,
              data: {
                product_id: String(f.get("product_id") ?? ""),
                code: String(f.get("code") ?? ""),
                warehouse_id: String(f.get("warehouse_id") ?? ""),
                quantity: String(f.get("quantity") ?? "").trim(),
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>
              {t("supply.receiveFrom", { code: transfer.from_lot.code })}
            </DialogTitle>
            <DialogDescription>
              {t("supply.receiveText", { quantity: transfer.quantity })}
            </DialogDescription>
          </DialogHeader>
          <Field
            id="rcv-product"
            label={t("lots.product")}
            errors={errors.product_id}
          >
            <select name="product_id" className={selectClass} required>
              {products.data?.data.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
          <Field
            id="rcv-code"
            label={t("supply.newLotCode")}
            errors={errors.code}
          >
            <Input name="code" required />
          </Field>
          <Field
            id="rcv-warehouse"
            label={t("stock.warehouse")}
            errors={errors.warehouse_id}
          >
            <select name="warehouse_id" className={selectClass} required>
              {warehouses.data?.data.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </Field>
          <Field
            id="rcv-quantity"
            label={t("supply.quantity")}
            errors={errors.quantity}
          >
            <Input
              name="quantity"
              inputMode="decimal"
              defaultValue={transfer.quantity}
              required
            />
          </Field>
          <ApiError error={receive.error} fallback={t("common.error")} />
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              {t("actions.cancel")}
            </DialogClose>
            <Button type="submit" disabled={receive.isPending}>
              {t("supply.receive")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
