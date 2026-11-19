"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import {
  getWarehousesListQueryKey,
  useWarehousesCreate,
  useWarehousesDelete,
  useWarehousesList,
  useWarehousesUpdate,
} from "@verzo/api-client/endpoints/warehouses"
import type { WarehouseResource } from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Badge } from "@verzo/ui/components/badge"
import { Button } from "@verzo/ui/components/button"
import { ConfirmDialog } from "@verzo/ui/components/confirm-dialog"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@verzo/ui/components/dialog"
import { Input } from "@verzo/ui/components/input"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@verzo/ui/components/table"
import { Textarea } from "@verzo/ui/components/textarea"
import { Field } from "@/components/field"
import { GeneralStock } from "@/components/general-stock"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { Done, Empty, LoadError, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

export default function WarehousesPage() {
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  return <WarehouseList key={workspaceId} workspaceId={workspaceId} />
}

function WarehouseList({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations()
  const queryClient = useQueryClient()
  const rights = useRights()
  const [page, setPage] = React.useState(1)
  const warehouses = useWarehousesList(
    workspaceId,
    { page, roots_only: false },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const done = useDone()
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getWarehousesListQueryKey(workspaceId),
    })
  const saved = (message: string) => {
    done.show(message)
    return refresh()
  }
  const remove = useWarehousesDelete({
    mutation: {
      onMutate: done.clear,
      onSuccess: (_answer, { warehouse }) => {
        const rows = warehouses.data?.data
        if (page > 1 && rows?.length === 1 && rows[0]?.id === warehouse)
          setPage(page - 1)
        return saved(t("warehouses.removed"))
      },
    },
  })
  const columns = [
    { label: t("warehouses.name") },
    { label: t("warehouses.description") },
  ]

  return (
    <>
      <PageHead
        title={t("warehouses.title")}
        back={
          rights.may("stock.list")
            ? { href: "/stock", label: t("stock.title") }
            : undefined
        }
        actions={
          rights.may("warehouse.create") ? (
            <WarehouseDialog
              workspaceId={workspaceId}
              onSaved={() => saved(t("warehouses.created"))}
            />
          ) : null
        }
      />
      <Done message={done.message} />
      <ApiError
        error={remove.error}
        fallback={t("common.error")}
        codes={{ warehouse_holds_stock: t("warehouses.holdsStock") }}
      />
      <ListCard
        pager={
          warehouses.data ? (
            <ListPager meta={warehouses.data.meta} onPage={setPage} />
          ) : null
        }
      >
        {warehouses.isPending ? (
          <ListSkeleton columns={columns} />
        ) : warehouses.isError ? (
          <CardState>
            <LoadError
              error={warehouses.error}
              onRetry={() => void warehouses.refetch()}
            />
          </CardState>
        ) : warehouses.data.meta.total === 0 ? (
          <CardState>
            <Empty title={t("warehouses.empty")} />
            <GeneralStock
              workspaceId={workspaceId}
              onReady={() => done.show(t("warehouses.generalReady"))}
            />
          </CardState>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                {columns.map((column) => (
                  <TableHead key={column.label}>{column.label}</TableHead>
                ))}
                <TableHead>
                  <span className="sr-only">{t("warehouses.actions")}</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {warehouses.data.data.map((warehouse) => (
                <TableRow key={warehouse.id}>
                  <TableCell>
                    <div className="flex flex-wrap items-center gap-2 font-semibold">
                      {warehouse.name}
                      {warehouse.default_warehouse ? (
                        <Badge>{t("warehouses.default")}</Badge>
                      ) : null}
                    </div>
                  </TableCell>
                  <TableCell data-m="sub" className="text-muted-foreground">
                    {warehouse.description}
                  </TableCell>
                  <TableCell data-m="badge">
                    <div className="flex flex-wrap justify-end gap-2">
                      {rights.may("warehouse.update") ? (
                        <WarehouseDialog
                          warehouse={warehouse}
                          workspaceId={workspaceId}
                          onSaved={() => saved(t("warehouses.saved"))}
                        />
                      ) : null}
                      {rights.may("warehouse.delete") ? (
                        <ConfirmDialog
                          trigger={t("warehouses.remove")}
                          title={t("warehouses.removeTitle", {
                            name: warehouse.name,
                          })}
                          description={t("warehouses.removeText")}
                          confirm={t("warehouses.remove")}
                          cancel={t("actions.cancel")}
                          disabled={remove.isPending}
                          onConfirm={() =>
                            remove.mutate({
                              workspace: workspaceId,
                              warehouse: warehouse.id,
                            })
                          }
                        />
                      ) : null}
                    </div>
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

function WarehouseDialog({
  warehouse,
  workspaceId,
  onSaved,
}: {
  warehouse?: WarehouseResource
  workspaceId: string
  onSaved: () => unknown
}) {
  const t = useTranslations()
  const [open, setOpen] = React.useState(false)
  const id = React.useId()
  const mutation = {
    onSuccess: () => {
      setOpen(false)
      return onSaved()
    },
  }
  const create = useWarehousesCreate({ mutation })
  const update = useWarehousesUpdate({ mutation })
  const save = warehouse ? update : create
  const errors = fieldErrors(save.error)

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        setOpen(nextOpen)
        if (!nextOpen) save.reset()
      }}
    >
      <DialogTrigger
        render={<Button variant={warehouse ? "outline" : "default"} />}
      >
        {t(warehouse ? "warehouses.edit" : "warehouses.new")}
      </DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault()
            const form = new FormData(event.currentTarget)
            const data = {
              name: String(form.get("name") ?? "").trim(),
              description: String(form.get("description") ?? "").trim() || null,
            }
            if (warehouse)
              update.mutate({
                workspace: workspaceId,
                warehouse: warehouse.id,
                data,
              })
            else create.mutate({ workspace: workspaceId, data })
          }}
        >
          <DialogHeader>
            <DialogTitle>
              {warehouse
                ? t("warehouses.editTitle", { name: warehouse.name })
                : t("warehouses.new")}
            </DialogTitle>
          </DialogHeader>
          <Field
            id={`${id}-name`}
            label={t("warehouses.name")}
            errors={errors.name ?? errors.slug}
          >
            <Input
              name="name"
              defaultValue={warehouse?.name}
              required
              maxLength={150}
            />
          </Field>
          <Field
            id={`${id}-description`}
            label={t("warehouses.description")}
            errors={errors.description}
          >
            <Textarea
              name="description"
              defaultValue={warehouse?.description ?? ""}
            />
          </Field>
          <ApiError error={save.error} fallback={t("common.error")} />
          <DialogFooter>
            <Button type="submit" disabled={save.isPending}>
              {t("actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
