"use client"

import * as React from "react"
import Link from "next/link"
import { useFormatter, useTranslations } from "next-intl"
import { Plus } from "lucide-react"

import {
  useIntegrationList,
  useIntegrationWrite,
} from "@verzo/api-client/business-approvals"
import {
  integrationsConnectionsCreate,
  integrationsConnectionsDisconnect,
  integrationsConnectionsTest,
  integrationsConnectionsUpdate,
  integrationsPairsActivate,
  integrationsPairsCreate,
  integrationsPairsPause,
} from "@verzo/api-client/endpoints/integrations"
import { fieldErrors } from "@verzo/ui/components/api-error"
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
import { EmptyState } from "@verzo/ui/components/empty-state"
import { Input } from "@verzo/ui/components/input"
import { Notice } from "@verzo/ui/components/notice"
import {
  errorText,
  label,
} from "@verzo/ui/components/business-approvals/labels"
import type {
  ConnectionView,
  PairView,
} from "@verzo/ui/components/business-approvals/types"
import { Field, selectClass } from "@/components/field"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"

// The API keeps a stored secret when it gets this mask back (contracts.md §5.2, K18).
const MASK = "********"

/** Odoo and NetSuite connections, and the workflows (pairs) between them. */
export default function IntegrationsPage() {
  const t = useTranslations("businessApprovals")
  const manage = useCan("integration.manage")
  const connections = useIntegrationList<ConnectionView>("connections")
  const pairs = useIntegrationList<PairView>("pairs")
  const write = useIntegrationWrite()
  const done = useDone()
  const [disconnecting, setDisconnecting] = React.useState<{
    c: ConnectionView
    open: boolean
  }>()
  const run = (call: (ws: string) => Promise<unknown>, message: string) => {
    done.clear()
    write.mutate(call, { onSuccess: () => done.show(t(message)) })
  }
  const byId = new Map((connections.data?.data ?? []).map((c) => [c.id, c]))

  return (
    <>
      <PageHead
        title={t("settings.title")}
        description={t("settings.intro")}
        actions={
          manage ? (
            <ConnectionDialog
              onDone={() => done.show(t("settings.connectionAdded"))}
            />
          ) : null
        }
      />
      <Done message={done.message} />
      {write.error ? (
        <Notice variant="error" title={errorText(t, write.error)} />
      ) : null}

      <h2 className="text-lg font-medium">{t("settings.connections")}</h2>
      <ListCard>
        {connections.isPending ? (
          <ListSkeleton columns={[{ label: "" }, { label: "" }]} />
        ) : connections.isError ? (
          <CardState>
            <LoadError
              error={connections.error}
              onRetry={() => void connections.refetch()}
            />
          </CardState>
        ) : connections.data.data.length === 0 ? (
          <CardState>
            <EmptyState
              title={t("settings.noConnections")}
              description={t("settings.noConnectionsText")}
            />
          </CardState>
        ) : (
          <ul className="divide-y">
            {connections.data.data.map((c) => (
              <li key={c.id} className="flex flex-col gap-2 p-4">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{c.label}</span>
                  <span className="text-sm text-muted-foreground">
                    {label(t, "provider", c.provider)}
                  </span>
                  <StatusBadge
                    status={c.status === "active" ? "active" : "pending"}
                    tone={
                      c.status === "active"
                        ? "teal"
                        : c.status === "draft"
                          ? "slate"
                          : c.status === "degraded"
                            ? "gold"
                            : "red"
                    }
                    label={label(t, "connectionStatus", c.status)}
                  />
                </div>
                {c.status_reason ? (
                  <p className="text-sm">
                    {label(t, "connectionReason", c.status_reason)}
                  </p>
                ) : null}
                <SecretLine c={c} />
                <Probe c={c} />
                {manage ? (
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      className="max-md:min-h-11"
                      aria-label={`${t("settings.test")}: ${c.label}`}
                      disabled={write.isPending}
                      onClick={() =>
                        run(
                          (ws) => integrationsConnectionsTest(ws, c.id),
                          "settings.tested"
                        )
                      }
                    >
                      {t("settings.test")}
                    </Button>
                    <ConnectionDialog
                      connection={c}
                      onDone={() => done.show(t("settings.connectionSaved"))}
                    />
                    {c.status !== "disconnected" ? (
                      <Button
                        variant="outline"
                        className="max-md:min-h-11"
                        aria-label={`${t("settings.disconnect")}: ${c.label}`}
                        onClick={() => setDisconnecting({ c, open: true })}
                      >
                        {t("settings.disconnect")}
                      </Button>
                    ) : null}
                  </div>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </ListCard>

      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-lg font-medium">{t("settings.pairs")}</h2>
        {manage && connections.data?.data.length ? (
          <PairDialog
            connections={connections.data.data}
            onDone={() => done.show(t("settings.pairAdded"))}
          />
        ) : null}
      </div>
      <ListCard>
        {pairs.isPending ? (
          <ListSkeleton columns={[{ label: "" }, { label: "" }]} />
        ) : pairs.isError ? (
          <CardState>
            <LoadError
              error={pairs.error}
              onRetry={() => void pairs.refetch()}
            />
          </CardState>
        ) : pairs.data.data.length === 0 ? (
          <CardState>
            <EmptyState
              title={t("settings.noPairs")}
              description={t("settings.noPairsText")}
            />
          </CardState>
        ) : (
          <ul className="divide-y">
            {pairs.data.data.map((p) => {
              const name = t("settings.pairName", {
                source: byId.get(p.source_connection_id)?.label ?? "Odoo",
                target: byId.get(p.target_connection_id)?.label ?? "NetSuite",
              })
              return (
                <li key={p.id} className="flex flex-col gap-2 p-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{name}</span>
                    <StatusBadge
                      status={p.status}
                      tone={
                        p.status === "active"
                          ? "teal"
                          : p.status === "paused"
                            ? "gold"
                            : "slate"
                      }
                      label={label(t, "pairStatus", p.status)}
                    />
                  </div>
                  {p.paused_reason ? (
                    <p className="text-sm">
                      {label(t, "pausedReason", p.paused_reason)}
                    </p>
                  ) : null}
                  <div className="flex flex-wrap gap-2">
                    <Link
                      href={`/settings/integrations/${p.id}/mappings`}
                      className="inline-flex items-center rounded-lg border px-2.5 text-sm font-medium max-md:min-h-11 md:h-8"
                    >
                      {t("settings.mappings")}
                    </Link>
                    {manage && p.status !== "active" ? (
                      <Button
                        className="max-md:min-h-11"
                        aria-label={`${t("settings.activate")}: ${name}`}
                        disabled={write.isPending}
                        onClick={() =>
                          run(
                            (ws) => integrationsPairsActivate(ws, p.id),
                            "settings.activated"
                          )
                        }
                      >
                        {t("settings.activate")}
                      </Button>
                    ) : null}
                    {manage && p.status === "active" ? (
                      <Button
                        variant="outline"
                        className="max-md:min-h-11"
                        aria-label={`${t("settings.pause")}: ${name}`}
                        disabled={write.isPending}
                        onClick={() =>
                          run(
                            (ws) => integrationsPairsPause(ws, p.id),
                            "settings.paused"
                          )
                        }
                      >
                        {t("settings.pause")}
                      </Button>
                    ) : null}
                  </div>
                </li>
              )
            })}
          </ul>
        )}
      </ListCard>

      <ConfirmDialog
        open={disconnecting?.open ?? false}
        onOpenChange={(open) => setDisconnecting((d) => d && { ...d, open })}
        title={t("settings.disconnectTitle", {
          label: disconnecting?.c.label ?? "",
        })}
        description={t("settings.disconnectText")}
        confirm={t("settings.disconnect")}
        cancel={t("card.cancel")}
        disabled={write.isPending}
        onConfirm={() =>
          disconnecting &&
          run(
            (ws) => integrationsConnectionsDisconnect(ws, disconnecting.c.id),
            "settings.disconnected"
          )
        }
      />
    </>
  )
}

/** Whether a secret is stored: only ever the mask or "not stored", never the value. */
function SecretLine({ c }: { c: ConnectionView }) {
  const t = useTranslations("businessApprovals")
  const name = c.provider === "odoo" ? "api_key" : "private_key"
  const stored = Boolean(c.secrets[name])
  return (
    <p className="text-sm text-muted-foreground">
      {t(c.provider === "odoo" ? "settings.apiKey" : "settings.privateKey")}:{" "}
      {stored ? (
        <span className="font-mono" aria-label={t("settings.secretStored")}>
          {MASK}
        </span>
      ) : (
        t("settings.secretNone")
      )}
    </p>
  )
}

/** The last test: every check passed, or each failed check in the API's readable words. */
function Probe({ c }: { c: ConnectionView }) {
  const t = useTranslations("businessApprovals")
  const format = useFormatter()
  if (!c.last_probe || !c.last_probe_at)
    return (
      <p className="text-sm text-muted-foreground">
        {t("settings.neverTested")}
      </p>
    )
  const failed = c.last_probe.filter((p) => !p.ok)
  return (
    <div className="flex flex-col gap-1 text-sm" data-testid="probe">
      <p className="text-muted-foreground">
        {t("settings.lastTest", {
          when: format.dateTime(new Date(c.last_probe_at), {
            dateStyle: "medium",
            timeStyle: "short",
          }),
        })}
      </p>
      {failed.length === 0 ? (
        <p>{t("settings.checksPassed", { n: c.last_probe.length })}</p>
      ) : (
        failed.map((p) => (
          <Notice
            key={p.key}
            variant="warning"
            live={false}
            title={t("settings.checkFailed")}
          >
            {p.detail ?? label(t, "connectionReason", "probe_failed")}
          </Notice>
        ))
      )}
    </div>
  )
}

/**
 * Add a connection, or edit one. The secret input is write-only: it starts empty, and left
 * empty on an edit it sends the mask back, so the API keeps the stored secret.
 */
function ConnectionDialog({
  connection,
  onDone,
}: {
  connection?: ConnectionView
  onDone: () => void
}) {
  const t = useTranslations("businessApprovals")
  const [open, setOpen] = React.useState(false)
  const [provider, setProvider] = React.useState<"odoo" | "netsuite">(
    connection?.provider ?? "odoo"
  )
  const write = useIntegrationWrite()
  const errors = fieldErrors(write.error)
  const cfg = (connection?.config ?? {}) as Record<string, string | null>
  const secretName = provider === "odoo" ? "api_key" : "private_key"
  const stored = Boolean(connection?.secrets[secretName])
  const title = connection
    ? t("settings.editConnection")
    : t("settings.addConnection")
  const onOpenChange = (o: boolean) => {
    setOpen(o)
    if (!o) write.reset()
  }
  const text = (f: FormData, k: string) => String(f.get(k) ?? "").trim()

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger
        render={
          connection ? (
            <Button variant="outline" className="max-md:min-h-11" />
          ) : (
            <Button />
          )
        }
        aria-label={connection ? `${title}: ${connection.label}` : undefined}
      >
        {connection ? null : <Plus aria-hidden="true" strokeWidth={1.75} />}
        {title}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <form
          className="flex flex-col gap-3"
          autoComplete="off"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const secret = String(f.get("secret") ?? "")
            const config =
              provider === "odoo"
                ? {
                    base_url: text(f, "base_url"),
                    database: text(f, "database") || null,
                  }
                : {
                    rest_domain: text(f, "rest_domain"),
                    environment: text(f, "environment"),
                    client_id: text(f, "client_id"),
                    certificate_id: text(f, "certificate_id"),
                    alg: "ES256",
                  }
            const data = {
              ...(connection ? {} : { provider }),
              label: text(f, "label"),
              config,
              ...(secret
                ? { secrets: { [secretName]: secret } }
                : stored
                  ? { secrets: { [secretName]: MASK } }
                  : {}),
              credential_expires_at: text(f, "expires")
                ? new Date(`${text(f, "expires")}T00:00:00Z`).toISOString()
                : null,
            }
            write.mutate(
              (ws) =>
                connection
                  ? integrationsConnectionsUpdate(ws, connection.id, data)
                  : integrationsConnectionsCreate(ws, data),
              {
                onSuccess: () => {
                  onDone()
                  setOpen(false)
                },
              }
            )
          }}
        >
          {connection ? null : (
            <Field id="conn-provider" label={t("settings.provider")}>
              <select
                className={selectClass}
                value={provider}
                onChange={(e) =>
                  setProvider(e.target.value as "odoo" | "netsuite")
                }
              >
                <option value="odoo">{label(t, "provider", "odoo")}</option>
                <option value="netsuite">
                  {label(t, "provider", "netsuite")}
                </option>
              </select>
            </Field>
          )}
          <Field
            id="conn-label"
            label={t("settings.label")}
            errors={errors.label}
          >
            <Input name="label" defaultValue={connection?.label} required />
          </Field>
          {provider === "odoo" ? (
            <>
              <Field
                id="conn-base-url"
                label={t("settings.baseUrl")}
                errors={errors["config.base_url"]}
              >
                <Input
                  name="base_url"
                  type="url"
                  defaultValue={cfg.base_url ?? ""}
                  required
                />
              </Field>
              <Field
                id="conn-database"
                label={t("settings.database")}
                errors={errors["config.database"]}
              >
                <Input name="database" defaultValue={cfg.database ?? ""} />
              </Field>
            </>
          ) : (
            <>
              <Field
                id="conn-rest-domain"
                label={t("settings.restDomain")}
                errors={errors["config.rest_domain"]}
              >
                <Input
                  name="rest_domain"
                  defaultValue={cfg.rest_domain ?? ""}
                  required
                />
              </Field>
              <Field
                id="conn-environment"
                label={t("settings.environment")}
                errors={errors["config.environment"]}
              >
                <select
                  name="environment"
                  className={selectClass}
                  defaultValue={cfg.environment ?? "production"}
                >
                  <option value="production">{t("settings.production")}</option>
                  <option value="sandbox">{t("settings.sandbox")}</option>
                </select>
              </Field>
              <Field
                id="conn-client-id"
                label={t("settings.clientId")}
                errors={errors["config.client_id"]}
              >
                <Input
                  name="client_id"
                  defaultValue={cfg.client_id ?? ""}
                  required
                />
              </Field>
              <Field
                id="conn-certificate-id"
                label={t("settings.certificateId")}
                errors={errors["config.certificate_id"]}
              >
                <Input
                  name="certificate_id"
                  defaultValue={cfg.certificate_id ?? ""}
                  required
                />
              </Field>
            </>
          )}
          <Field
            id="conn-secret"
            label={t(
              provider === "odoo" ? "settings.apiKey" : "settings.privateKey"
            )}
            errors={errors[`secrets.${secretName}`]}
          >
            <Input
              name="secret"
              type="password"
              autoComplete="new-password"
              placeholder={stored ? t("settings.secretStored") : undefined}
              required={!stored}
            />
          </Field>
          <Field
            id="conn-expires"
            label={t("settings.expires")}
            errors={errors.credential_expires_at}
          >
            <Input
              name="expires"
              type="date"
              defaultValue={
                connection?.credential_expires_at?.slice(0, 10) ?? ""
              }
            />
          </Field>
          {write.error && !Object.keys(errors).length ? (
            <Notice variant="error" title={errorText(t, write.error)} />
          ) : null}
          <DialogFooter>
            <Button type="submit" disabled={write.isPending}>
              {t("settings.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** Connect an Odoo connection to a NetSuite connection (WF-01). */
function PairDialog({
  connections,
  onDone,
}: {
  connections: ConnectionView[]
  onDone: () => void
}) {
  const t = useTranslations("businessApprovals")
  const [open, setOpen] = React.useState(false)
  const write = useIntegrationWrite()
  const errors = fieldErrors(write.error)
  const of = (p: string) => connections.filter((c) => c.provider === p)
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) write.reset()
      }}
    >
      <DialogTrigger
        render={<Button variant="outline" className="max-md:min-h-11" />}
      >
        <Plus aria-hidden="true" strokeWidth={1.75} />
        {t("settings.addPair")}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("settings.addPair")}</DialogTitle>
        </DialogHeader>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            write.mutate(
              (ws) =>
                integrationsPairsCreate(ws, {
                  workflow_key: "wf01_purchase",
                  source_connection_id: String(f.get("source")),
                  target_connection_id: String(f.get("target")),
                }),
              {
                onSuccess: () => {
                  onDone()
                  setOpen(false)
                },
              }
            )
          }}
        >
          {(
            [
              ["source", "odoo", "settings.source"],
              ["target", "netsuite", "settings.target"],
            ] as const
          ).map(([name, provider, text]) => (
            <Field
              key={name}
              id={`pair-${name}`}
              label={t(text)}
              errors={errors[`${name}_connection_id`]}
            >
              <select name={name} className={selectClass} required>
                {of(provider).map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.label}
                  </option>
                ))}
              </select>
            </Field>
          ))}
          {write.error && !Object.keys(errors).length ? (
            <Notice variant="error" title={errorText(t, write.error)} />
          ) : null}
          <DialogFooter>
            <Button type="submit" disabled={write.isPending}>
              {t("settings.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
