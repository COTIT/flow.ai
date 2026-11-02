"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getTillUsersListQueryKey,
  useTillUsersCreate,
  useTillUsersList,
  useTillUsersUpdate,
} from "@genesy/api-client/endpoints/till"
import type { TillUserResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import {
  Dialog,
  DialogClose,
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
import { Field } from "@/components/field"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"

export default function TillUsersPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const users = useTillUsersList(workspaceId, {
    query: { enabled: Boolean(workspaceId) },
  })
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getTillUsersListQueryKey(workspaceId),
    })
  const done = useDone()
  const update = useTillUsersUpdate({
    mutation: { onMutate: done.clear, onSuccess: refresh },
  })

  return (
    <>
      <Done message={done.message} />
      <div className="flex justify-end">
        <PinDialog
          onDone={() => {
            done.show(t("done.tillUserAdded"))
            void refresh()
          }}
        />
      </div>
      <ApiError error={update.error} fallback={t("common.error")} />
      {users.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : users.isError ? (
        <LoadError error={users.error} onRetry={() => void users.refetch()} />
      ) : users.data.data.length === 0 ? (
        <Empty title={t("till.noUsers")} />
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("appKeys.name")}</TableHead>
              <TableHead>{t("webhooks.state")}</TableHead>
              <TableHead>
                <span className="sr-only">{t("appKeys.actions")}</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {users.data.data.map((u) => (
              <TableRow key={u.id}>
                <TableCell className="font-medium">{u.name}</TableCell>
                <TableCell>
                  <div className="flex gap-1">
                    <StatusBadge
                      status={u.active ? "active" : "inactive"}
                      label={
                        u.active ? t("webhooks.active") : t("till.inactive")
                      }
                    />
                    {u.locked ? (
                      <StatusBadge status="recalled" label={t("till.locked")} />
                    ) : null}
                  </div>
                </TableCell>
                <TableCell>
                  <div className="flex justify-end gap-2">
                    <PinDialog
                      user={u}
                      onDone={() => {
                        done.show(t("done.pinChanged", { name: u.name }))
                        void refresh()
                      }}
                    />
                    {u.active ? (
                      <ConfirmDialog
                        trigger={t("till.deactivate")}
                        title={t("till.deactivateTitle", { name: u.name })}
                        description={t("till.deactivateText")}
                        confirm={t("till.deactivate")}
                        cancel={t("actions.cancel")}
                        disabled={update.isPending}
                        onConfirm={() =>
                          update.mutate(
                            {
                              workspace: workspaceId,
                              user: u.id,
                              data: { active: false },
                            },
                            {
                              onSuccess: () =>
                                done.show(
                                  t("done.deactivated", { name: u.name })
                                ),
                            }
                          )
                        }
                      />
                    ) : (
                      <Button
                        variant="outline"
                        disabled={update.isPending}
                        onClick={() =>
                          update.mutate(
                            {
                              workspace: workspaceId,
                              user: u.id,
                              data: { active: true },
                            },
                            {
                              onSuccess: () =>
                                done.show(
                                  t("done.activated", { name: u.name })
                                ),
                            }
                          )
                        }
                      >
                        {t("till.activate")}
                      </Button>
                    )}
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </>
  )
}

/** New till user (no `user`) or a PIN reset for one; the PIN is sent once and never shown. */
function PinDialog({
  user,
  onDone,
}: {
  user?: TillUserResource
  onDone: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const done = {
    onSuccess: () => {
      onDone()
      setOpen(false)
    },
  }
  const create = useTillUsersCreate({ mutation: done })
  const update = useTillUsersUpdate({ mutation: done })
  const mutation = user ? update : create
  const errors = fieldErrors(mutation.error)
  const title = user
    ? t("till.resetPinFor", { name: user.name })
    : t("till.newUser")

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) mutation.reset()
      }}
    >
      <DialogTrigger
        render={<Button variant={user ? "outline" : "default"} />}
        aria-label={user ? title : undefined}
      >
        {user ? t("till.resetPin") : t("till.newUser")}
      </DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const pin = String(f.get("pin") ?? "")
            if (user)
              update.mutate({
                workspace: workspaceId,
                user: user.id,
                data: { pin },
              })
            else
              create.mutate({
                workspace: workspaceId,
                data: { name: String(f.get("name") ?? ""), pin },
              })
          }}
        >
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
          </DialogHeader>
          {user ? null : (
            <Field id="tu-name" label={t("appKeys.name")} errors={errors.name}>
              <Input name="name" required />
            </Field>
          )}
          <Field id="tu-pin" label={t("till.pin")} errors={errors.pin}>
            <Input
              name="pin"
              type="password"
              inputMode="numeric"
              autoComplete="new-password"
              required
            />
          </Field>
          <ApiError error={mutation.error} fallback={t("common.error")} />
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              {t("actions.cancel")}
            </DialogClose>
            <Button type="submit" disabled={mutation.isPending}>
              {t("actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
