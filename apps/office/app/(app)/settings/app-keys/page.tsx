"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"
import { Plus } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import {
  getAppKeysListQueryKey,
  useAppKeysCreate,
  useAppKeysList,
  useAppKeysRevoke,
} from "@genesy/api-client/endpoints/app-keys"
import {
  AppKeyIssuableScope,
  AppKeyKind,
} from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Badge } from "@genesy/ui/components/badge"
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
import { formatDateTime } from "@genesy/ui/lib/format"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Choices, Field, selectClass } from "@/components/field"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { SecretReveal } from "@/components/secret-reveal"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"
import { labelKey } from "@/lib/labels"

export default function AppKeysPage() {
  const t = useTranslations()
  const locale = useLocale()
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
  const done = useDone()
  const revoke = useAppKeysRevoke({
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        done.show(t("done.keyRevoked"))
        return refresh()
      },
    },
  })
  // The key being confirmed; kept while the dialog closes so its title does not blank.
  const [revoking, setRevoking] = React.useState<{
    id: string
    name: string
    open: boolean
  }>()
  const when = (s: string | null) => (s ? formatDateTime(s, locale) : "—")
  const columns = [
    { label: t("appKeys.name") },
    { label: t("appKeys.kind") },
    { label: t("appKeys.scopes") },
    { label: t("appKeys.prefix") },
    { label: t("appKeys.lastUsed") },
    { label: t("appKeys.actions"), right: true },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("appKeys.title")}
        description={t("appKeys.intro")}
        actions={<CreateKey onCreated={refresh} />}
      />
      <Done message={done.message} />
      <ApiError error={revoke.error} fallback={t("common.error")} />
      <ListCard
        pager={
          keys.data?.data.length ? (
            <ListPager meta={keys.data.meta} onPage={setPage} />
          ) : null
        }
      >
        {keys.isPending ? (
          <ListSkeleton columns={columns} />
        ) : keys.isError ? (
          <CardState>
            <LoadError error={keys.error} onRetry={() => void keys.refetch()} />
          </CardState>
        ) : keys.data.data.length === 0 ? (
          <CardState>
            <Empty title={t("appKeys.empty")} />
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
              {keys.data.data.map((k) => (
                <TableRow key={k.id}>
                  <TableCell className="font-semibold">{k.name}</TableCell>
                  <TableCell data-m="sub">
                    {k.revoked_at ? (
                      <StatusBadge
                        status="revoked"
                        label={t("appKeys.revoked")}
                      />
                    ) : (
                      <Badge variant="teal">
                        {t(`appKeys.kinds.${labelKey(k.kind)}`)}
                      </Badge>
                    )}
                  </TableCell>
                  <TableCell data-m="hide">
                    <span className="flex flex-wrap gap-1">
                      {k.scopes.map((s) => (
                        <Badge
                          key={s}
                          variant="slate"
                          title={t(`appKeys.scopeHints.${labelKey(s)}`)}
                        >
                          {t(`appKeys.scopeNames.${labelKey(s)}`)}
                        </Badge>
                      ))}
                    </span>
                  </TableCell>
                  <TableCell data-m="hide" className="font-mono tracking-[0.04em]">
                    {k.prefix}
                  </TableCell>
                  <TableCell
                    data-m="hide"
                    className="text-muted-foreground tabular-nums"
                  >
                    {when(k.last_used_at)}
                  </TableCell>
                  <TableCell data-m="badge" className="text-right">
                    {k.revoked_at ? (
                      <span className="text-xs text-muted-foreground">—</span>
                    ) : (
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={revoke.isPending}
                        onClick={() =>
                          setRevoking({ id: k.id, name: k.name, open: true })
                        }
                      >
                        {t("appKeys.revoke")}
                      </Button>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
      <ConfirmDialog
        open={revoking?.open ?? false}
        onOpenChange={(open) => setRevoking((r) => r && { ...r, open })}
        title={t("appKeys.revokeTitle", { name: revoking?.name ?? "" })}
        description={t("appKeys.revokeText")}
        confirm={t("appKeys.revoke")}
        cancel={t("actions.cancel")}
        disabled={revoke.isPending}
        onConfirm={() =>
          revoking && revoke.mutate({ workspace: workspaceId, key: revoking.id })
        }
      />
    </>
  )
}

function CreateKey({ onCreated }: { onCreated: () => void }) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const create = useAppKeysCreate({ mutation: { onSuccess: onCreated } })
  const errors = fieldErrors(create.error)
  const onOpenChange = (o: boolean) => {
    setOpen(o)
    // Forget the secret once the dialog closes: it is never shown again.
    if (!o) create.reset()
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger render={<Button />}>
        <Plus aria-hidden="true" strokeWidth={1.75} />
        {t("appKeys.new")}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {create.data ? t("appKeys.created") : t("appKeys.new")}
          </DialogTitle>
        </DialogHeader>
        {create.data ? (
          <SecretReveal
            secret={create.data.data.key}
            hint={t("appKeys.secretHint")}
            onSaved={() => onOpenChange(false)}
          />
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
                  kind: String(f.get("kind") ?? "") as AppKeyKind,
                  scopes: f.getAll("scopes").map(String) as AppKeyIssuableScope[],
                },
              })
            }}
          >
            <Field id="key-name" label={t("appKeys.name")} errors={errors.name}>
              <Input name="name" required />
            </Field>
            <Field id="key-kind" label={t("appKeys.kind")} errors={errors.kind}>
              <select name="kind" className={selectClass} required defaultValue="">
                <option value="" disabled>
                  {t("appKeys.chooseKind")}
                </option>
                {Object.values(AppKeyKind).map((k) => (
                  <option key={k} value={k}>
                    {t(`appKeys.kinds.${k}`)}
                  </option>
                ))}
              </select>
            </Field>
            <Choices
              name="scopes"
              legend={t("appKeys.scopes")}
              errors={errors.scopes}
              options={Object.values(AppKeyIssuableScope).map((s) => ({
                value: s,
                label: t(`appKeys.scopeNames.${labelKey(s)}`),
                hint: t(`appKeys.scopeHints.${labelKey(s)}`),
              }))}
            />
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
