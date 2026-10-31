"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getWebhooksListQueryKey,
  useWebhooksCreate,
  useWebhooksDelete,
  useWebhooksList,
  useWebhooksUpdate,
} from "@genesy/api-client/endpoints/webhooks"
import type { WebhookEndpointResource } from "@genesy/api-client/model"
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
  const remove = useWebhooksDelete({ mutation: { onSuccess: refresh } })

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <h1 className="mr-auto heading-2 text-lg">{t("webhooks.title")}</h1>
        <EndpointDialog onDone={refresh} />
      </div>
      <ApiError error={remove.error} fallback={t("common.error")} />
      {hooks.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : hooks.isError ? (
        <ApiError error={hooks.error} fallback={t("common.error")} />
      ) : hooks.data.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("webhooks.empty")}</p>
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("webhooks.url")}</TableHead>
                <TableHead>{t("webhooks.events")}</TableHead>
                <TableHead>{t("webhooks.state")}</TableHead>
                <TableHead>
                  <span className="sr-only">{t("appKeys.actions")}</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {hooks.data.data.map((h) => (
                <TableRow key={h.id}>
                  <TableCell className="max-w-64 truncate font-mono text-xs">
                    {h.url}
                  </TableCell>
                  <TableCell>{h.events.join(", ")}</TableCell>
                  <TableCell>
                    <StatusBadge
                      status={h.active ? "active" : "inactive"}
                      label={
                        h.active ? t("webhooks.active") : t("webhooks.inactive")
                      }
                    />
                  </TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-2">
                      <EndpointDialog endpoint={h} onDone={refresh} />
                      <ConfirmDialog
                        trigger={t("webhooks.delete")}
                        title={t("webhooks.deleteTitle")}
                        description={t("webhooks.deleteText", { url: h.url })}
                        confirm={t("webhooks.delete")}
                        cancel={t("actions.cancel")}
                        disabled={remove.isPending}
                        onConfirm={() =>
                          remove.mutate({
                            workspace: workspaceId,
                            endpoint: h.id,
                          })
                        }
                      />
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pager
            page={hooks.data.meta.current_page}
            lastPage={hooks.data.meta.last_page}
            summary={t("pager.summary", {
              from: hooks.data.meta.from ?? 0,
              to: hooks.data.meta.to ?? 0,
              total: hooks.data.meta.total,
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

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) mutation.reset()
      }}
    >
      <DialogTrigger
        render={<Button variant={endpoint ? "outline" : "default"} />}
        aria-label={endpoint ? `${title}: ${endpoint.url}` : undefined}
      >
        {title}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {create.data ? (
          <SecretReveal secret={create.data.data.secret} />
        ) : (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              const f = new FormData(e.currentTarget)
              const data = {
                url: String(f.get("url") ?? ""),
                events: splitList(f.get("events")),
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
