"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getAppKeysListQueryKey,
  useAppKeysCreate,
  useAppKeysList,
  useAppKeysRevoke,
} from "@genesy/api-client/endpoints/app-keys"
import type { AppKeyIssuableScope, AppKeyKind } from "@genesy/api-client/model"
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
import { Field } from "@/components/field"
import { SecretReveal, splitList } from "@/components/secret-reveal"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"

export default function AppKeysPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [page, setPage] = React.useState(1)
  const queryClient = useQueryClient()
  const keys = useAppKeysList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getAppKeysListQueryKey(workspaceId),
    })
  const revoke = useAppKeysRevoke({ mutation: { onSuccess: refresh } })
  const when = (s: string | null) => (s ? new Date(s).toLocaleString() : "—")

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <h1 className="mr-auto heading-2 text-lg">{t("appKeys.title")}</h1>
        <CreateKey onCreated={refresh} />
      </div>
      <ApiError error={revoke.error} fallback={t("common.error")} />
      {keys.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : keys.isError ? (
        <LoadError error={keys.error} onRetry={() => void keys.refetch()} />
      ) : keys.data.data.length === 0 ? (
        <Empty title={t("appKeys.empty")} />
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("appKeys.name")}</TableHead>
                <TableHead>{t("appKeys.kind")}</TableHead>
                <TableHead>{t("appKeys.scopes")}</TableHead>
                <TableHead>{t("appKeys.prefix")}</TableHead>
                <TableHead>{t("appKeys.lastUsed")}</TableHead>
                <TableHead>
                  <span className="sr-only">{t("appKeys.actions")}</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {keys.data.data.map((k) => (
                <TableRow key={k.id}>
                  <TableCell className="font-medium">{k.name}</TableCell>
                  <TableCell>{k.kind}</TableCell>
                  <TableCell>{k.scopes.join(", ")}</TableCell>
                  <TableCell className="font-mono text-xs">
                    {k.prefix}
                  </TableCell>
                  <TableCell className="tabular-nums">
                    {when(k.last_used_at)}
                  </TableCell>
                  <TableCell className="text-right">
                    {k.revoked_at ? (
                      <StatusBadge
                        status="revoked"
                        label={t("appKeys.revoked")}
                      />
                    ) : (
                      <ConfirmDialog
                        trigger={t("appKeys.revoke")}
                        title={t("appKeys.revokeTitle", { name: k.name })}
                        description={t("appKeys.revokeText")}
                        confirm={t("appKeys.revoke")}
                        cancel={t("actions.cancel")}
                        disabled={revoke.isPending}
                        onConfirm={() =>
                          revoke.mutate({ workspace: workspaceId, key: k.id })
                        }
                      />
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pager
            page={keys.data.meta.current_page}
            lastPage={keys.data.meta.last_page}
            summary={t("pager.summary", {
              from: keys.data.meta.from ?? 0,
              to: keys.data.meta.to ?? 0,
              total: keys.data.meta.total,
            })}
            previous={t("pager.previous")}
            next={t("pager.next")}
            onPage={setPage}
          />
        </>
      )}
    </section>
  )
}

function CreateKey({ onCreated }: { onCreated: () => void }) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const create = useAppKeysCreate({ mutation: { onSuccess: onCreated } })
  const errors = fieldErrors(create.error)

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        // Forget the secret once the dialog closes: it is never shown again.
        if (!o) create.reset()
      }}
    >
      <DialogTrigger render={<Button />}>{t("appKeys.new")}</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("appKeys.new")}</DialogTitle>
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
                  // Free text; the API's 422 refuses unknown values.
                  kind: String(f.get("kind") ?? "") as AppKeyKind,
                  scopes: splitList(f.get("scopes")) as AppKeyIssuableScope[],
                },
              })
            }}
          >
            <Field id="key-name" label={t("appKeys.name")} errors={errors.name}>
              <Input name="name" required />
            </Field>
            <Field id="key-kind" label={t("appKeys.kind")} errors={errors.kind}>
              <Input name="kind" required />
            </Field>
            <Field
              id="key-scopes"
              label={t("appKeys.scopes")}
              errors={errors.scopes}
            >
              <Input name="scopes" required />
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
