"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getTillVaultsListQueryKey,
  useTillVaultsCreate,
  useTillVaultsList,
} from "@genesy/api-client/endpoints/till"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import {
  Dialog,
  DialogContent,
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
import { Field, selectClass } from "@/components/field"
import { CardState, ListCard, ListSkeleton } from "@/components/list"
import { useTillMoney } from "@/lib/till-money"
import { Done, Empty, LoadError, useDone } from "@/components/states"

export default function VaultsPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const { money, currencies } = useTillMoney()
  const vaults = useTillVaultsList(workspaceId, {
    query: { enabled: Boolean(workspaceId) },
  })
  const [open, setOpen] = React.useState(false)
  const done = useDone()
  const create = useTillVaultsCreate({
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        done.show(t("done.vaultAdded"))
        void queryClient.invalidateQueries({
          queryKey: getTillVaultsListQueryKey(workspaceId),
        })
        setOpen(false)
      },
    },
  })
  const errors = fieldErrors(create.error)
  const columns = [
    { label: t("appKeys.name") },
    { label: t("till.balance"), right: true },
  ]

  return (
    <>
      <Done message={done.message} />
      <ListCard
        filters={
          <div className="ml-auto">
            <Dialog
              open={open}
              onOpenChange={(o) => {
                setOpen(o)
                if (!o) create.reset()
              }}
            >
              <DialogTrigger render={<Button />}>
                {t("till.newVault")}
              </DialogTrigger>
              <DialogContent>
                <form
                  className="flex flex-col gap-3"
                  onSubmit={(e) => {
                    e.preventDefault()
                    const f = new FormData(e.currentTarget)
                    create.mutate({
                      workspace: workspaceId,
                      data: {
                        name: String(f.get("name") ?? ""),
                        currency_id: String(f.get("currency_id") ?? "") || null,
                      },
                    })
                  }}
                >
                  <DialogHeader>
                    <DialogTitle>{t("till.newVault")}</DialogTitle>
                  </DialogHeader>
                  <Field
                    id="vault-name"
                    label={t("appKeys.name")}
                    errors={errors.name}
                  >
                    <Input name="name" required />
                  </Field>
                  <Field
                    id="vault-currency"
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
              </DialogContent>
            </Dialog>
          </div>
        }
      >
        {vaults.isPending ? (
          <ListSkeleton columns={columns} />
        ) : vaults.isError ? (
          <CardState>
            <LoadError
              error={vaults.error}
              onRetry={() => void vaults.refetch()}
            />
          </CardState>
        ) : vaults.data.data.length === 0 ? (
          <CardState>
            <Empty title={t("till.noVaults")} />
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
              {vaults.data.data.map((v) => (
                <TableRow key={v.id}>
                  <TableCell className="font-semibold">{v.name}</TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {money(v.balance, v.currency_id)}
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
