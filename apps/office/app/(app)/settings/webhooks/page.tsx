"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"
import { Plus } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import {
  getWebhooksListQueryKey,
  useWebhooksCreate,
  useWebhooksDelete,
  useWebhooksList,
  useWebhooksUpdate,
} from "@genesy/api-client/endpoints/webhooks"
import type {
  WebhookEndpointResource,
  WebhookEvent,
} from "@genesy/api-client/model"
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Field } from "@/components/field"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { SecretReveal, splitList } from "@/components/secret-reveal"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"

export default function WebhooksPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [page, setPage] = React.useState(1)
  const queryClient = useQueryClient()
  const hooks = useWebhooksList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getWebhooksListQueryKey(workspaceId),
    })
  const done = useDone()
  const remove = useWebhooksDelete({
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        done.show(t("done.endpointDeleted"))
        return refresh()
      },
    },
  })
  // The endpoint being confirmed; kept while the dialog closes so its text does not blank.
  const [deleting, setDeleting] = React.useState<{
    id: string
    url: string
    open: boolean
  }>()
  const columns = [
    { label: t("webhooks.url") },
    { label: t("webhooks.events") },
    { label: t("webhooks.state") },
    { label: t("appKeys.actions"), right: true },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("webhooks.title")}
        description={t("webhooks.intro")}
        actions={
          <EndpointDialog
            onDone={() => {
              done.show(t("done.endpointAdded"))
              void refresh()
            }}
          />
        }
      />
      <Done message={done.message} />
      <ApiError error={remove.error} fallback={t("common.error")} />
      <ListCard
        pager={
          hooks.data?.data.length ? (
            <ListPager meta={hooks.data.meta} onPage={setPage} />
          ) : null
        }
      >
        {hooks.isPending ? (
          <ListSkeleton columns={columns} />
        ) : hooks.isError ? (
          <CardState>
            <LoadError
              error={hooks.error}
              onRetry={() => void hooks.refetch()}
            />
          </CardState>
        ) : hooks.data.data.length === 0 ? (
          <CardState>
            <Empty title={t("webhooks.empty")} />
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
              {hooks.data.data.map((h) => (
                <TableRow key={h.id}>
                  <TableCell className="max-w-64 truncate font-mono font-semibold tracking-[0.04em]">
                    {h.url}
                  </TableCell>
                  <TableCell data-m="sub" className="text-muted-foreground">
                    {h.events.join(", ")}
                  </TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={h.active ? "active" : "inactive"}
                      label={
                        h.active ? t("webhooks.active") : t("webhooks.inactive")
                      }
                    />
                  </TableCell>
                  <TableCell data-m="right">
                    <div className="flex justify-end gap-2">
                      <EndpointDialog
                        endpoint={h}
                        onDone={() => {
                          done.show(t("done.endpointSaved"))
                          void refresh()
                        }}
                      />
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={remove.isPending}
                        onClick={() =>
                          setDeleting({ id: h.id, url: h.url, open: true })
                        }
                      >
                        {t("webhooks.delete")}
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
      <ConfirmDialog
        open={deleting?.open ?? false}
        onOpenChange={(open) => setDeleting((d) => d && { ...d, open })}
        title={t("webhooks.deleteTitle")}
        description={t("webhooks.deleteText", { url: deleting?.url ?? "" })}
        confirm={t("webhooks.delete")}
        cancel={t("actions.cancel")}
        disabled={remove.isPending}
        onConfirm={() =>
          deleting &&
          remove.mutate({ workspace: workspaceId, endpoint: deleting.id })
        }
      />
    </>
  )
}

/** Create (no `endpoint`) or edit one endpoint. A new endpoint's secret shows once. */
function EndpointDialog({
  endpoint,
  onDone,
}: {
  endpoint?: WebhookEndpointResource
  onDone: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const create = useWebhooksCreate({ mutation: { onSuccess: onDone } })
  const update = useWebhooksUpdate({
    mutation: {
      onSuccess: () => {
        onDone()
        setOpen(false)
      },
    },
  })
  const mutation = endpoint ? update : create
  const errors = fieldErrors(mutation.error)
  const title = endpoint ? t("webhooks.edit") : t("webhooks.new")
  const onOpenChange = (o: boolean) => {
    setOpen(o)
    if (!o) mutation.reset()
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger
        render={
          endpoint ? <Button variant="outline" size="sm" /> : <Button />
        }
        aria-label={endpoint ? `${title}: ${endpoint.url}` : undefined}
      >
        {endpoint ? null : <Plus aria-hidden="true" strokeWidth={1.75} />}
        {title}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {create.data ? t("webhooks.created") : title}
          </DialogTitle>
        </DialogHeader>
        {create.data ? (
          <SecretReveal
            secret={create.data.data.secret}
            hint={t("webhooks.secretHint")}
            onSaved={() => onOpenChange(false)}
          />
        ) : (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              const f = new FormData(e.currentTarget)
              const data = {
                url: String(f.get("url") ?? ""),
                // Free text; the API's 422 refuses unknown events.
                events: splitList(f.get("events")) as WebhookEvent[],
                description: String(f.get("description") ?? "") || null,
              }
              if (endpoint)
                update.mutate({
                  workspace: workspaceId,
                  endpoint: endpoint.id,
                  data: { ...data, active: f.get("active") === "on" },
                })
              else create.mutate({ workspace: workspaceId, data })
            }}
          >
            <Field id="hook-url" label={t("webhooks.url")} errors={errors.url}>
              <Input
                name="url"
                type="url"
                defaultValue={endpoint?.url}
                required
              />
            </Field>
            <Field
              id="hook-events"
              label={t("webhooks.events")}
              errors={errors.events}
            >
              <Input
                name="events"
                defaultValue={endpoint?.events.join(", ")}
                required
              />
            </Field>
            <Field
              id="hook-description"
              label={t("webhooks.description")}
              errors={errors.description}
            >
              <Input
                name="description"
                defaultValue={endpoint?.description ?? ""}
              />
            </Field>
            {endpoint ? (
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  name="active"
                  defaultChecked={endpoint.active}
                />
                {t("webhooks.active")}
              </label>
            ) : null}
            <ApiError error={mutation.error} fallback={t("common.error")} />
            <DialogFooter>
              <Button type="submit" disabled={mutation.isPending}>
                {t("actions.save")}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
