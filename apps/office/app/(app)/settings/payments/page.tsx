"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useSession } from "@genesy/api-client/auth"
import {
  settingsGet,
  settingsUpdate,
} from "@genesy/api-client/endpoints/settings"
import { apiRequest } from "@genesy/api-client/mutator"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Checkbox } from "@genesy/ui/components/checkbox"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { Textarea } from "@genesy/ui/components/textarea"
import { Tile } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { PageHead } from "@/components/list"
import { Done, LoadError, NoAccess, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

type Slot = Partial<
  Record<
    | "merchant_id"
    | "username"
    | "password"
    | "callback_secret"
    | "previous_callback_secret",
    string | null
  >
>
type PaymentSettings = {
  active_provider?: string | null
  enabled?: boolean
  environment?: string
  payment_providers?: Record<string, unknown> & {
    globadyme?: { sandbox?: Slot; production?: Slot }
  }
}
type Integrations = Record<string, unknown> & { payment?: PaymentSettings }

export default function PaymentSettingsPage() {
  const workspace = useSession((s) => s.workspaceId)
  const token = useSession((s) => s.token)
  const rights = useRights()
  if (!workspace || !token || !rights.read) return null
  if (!rights.may("workspace_admin")) return <NoAccess />
  return <PaymentSettings key={`${workspace}:${token}`} workspace={workspace} />
}

function PaymentSettings({ workspace }: { workspace: string }) {
  const t = useTranslations()
  const queryClient = useQueryClient()
  const done = useDone()
  const queryKey = ["payment-settings", workspace]
  const settings = useQuery({
    queryKey,
    queryFn: ({ signal }) =>
      settingsGet(workspace, { type: "integrations" }, undefined, signal),
    retry: false,
  })
  const [retire, setRetire] = React.useState<"sandbox" | "production" | null>(
    null
  )
  const save = useMutation({
    mutationFn: (integrations: Integrations) =>
      settingsUpdate(workspace, { integrations }),
    onMutate: done.clear,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey })
      done.show(t("payments.settingsSaved"))
    },
  })
  const retirement = useMutation({
    mutationFn: (data: { reason: string; evidence: string }) =>
      apiRequest({
        url: `/workspace/${workspace}/payments/globadyme/${retire}/callback-secret/retire`,
        method: "POST",
        data,
      }),
    onSuccess: async () => {
      setRetire(null)
      await queryClient.invalidateQueries({ queryKey })
      done.show(t("payments.secretRetired"))
    },
  })
  const integrations = (settings.data?.data ?? {}) as Integrations
  const payment = integrations.payment ?? {}
  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("payments.settingsTitle")}
        description={t("payments.settingsIntro")}
      />
      <Done message={done.message} />
      {settings.isPending ? (
        <p role="status">{t("common.loading")}</p>
      ) : settings.isError ? (
        <LoadError
          error={settings.error}
          onRetry={() => void settings.refetch()}
        />
      ) : (
        <form
          key={settings.dataUpdatedAt}
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault()
            const form = new FormData(event.currentTarget)
            const providers = { ...payment.payment_providers }
            const globadyme = { ...providers.globadyme }
            for (const environment of ["sandbox", "production"] as const) {
              const slot = { ...globadyme[environment] }
              for (const field of [
                "merchant_id",
                "username",
                "password",
                "callback_secret",
              ] as const) {
                const value = String(form.get(`${environment}.${field}`) ?? "")
                slot[field] =
                  (field === "merchant_id" || field === "username"
                    ? value.trim()
                    : value) || null
              }
              globadyme[environment] = slot
            }
            providers.globadyme = globadyme
            save.mutate({
              ...integrations,
              payment: {
                ...payment,
                active_provider: String(form.get("provider")) || null,
                enabled: form.get("enabled") === "on",
                environment: String(form.get("environment")),
                payment_providers: providers,
              },
            })
          }}
        >
          <Tile
            id="payment-settings"
            title={t("payments.provider")}
            span="col-span-12"
          >
            <div className="grid gap-4 md:grid-cols-2">
              <Field id="payment-provider" label={t("payments.provider")}>
                <select
                  name="provider"
                  className={selectClass}
                  defaultValue={
                    payment.active_provider === "globadyme" ? "globadyme" : ""
                  }
                >
                  <option value="">{t("payments.legacy")}</option>
                  <option value="globadyme">Globadyme</option>
                </select>
              </Field>
              <Field id="payment-environment" label={t("payments.environment")}>
                <select
                  name="environment"
                  className={selectClass}
                  defaultValue={payment.environment ?? "sandbox"}
                >
                  <option value="sandbox">{t("payments.sandbox")}</option>
                  <option value="production" disabled>
                    {t("payments.production")}
                  </option>
                </select>
              </Field>
            </div>
            <label className="flex min-h-11 items-center gap-3 text-sm">
              <Checkbox
                name="enabled"
                defaultChecked={payment.enabled ?? false}
                className="size-5 after:-inset-3"
              />
              {t("payments.enabled")}
            </label>
            <Notice variant="warning" title={t("payments.productionBlocked")} />
            <p className="text-sm text-muted-foreground">
              {t("payments.identityHint")}
            </p>
          </Tile>
          {(["sandbox", "production"] as const).map((environment) => (
            <Tile
              key={environment}
              id={`payment-${environment}`}
              title={`Globadyme · ${t(`payments.${environment}`)}`}
              span="col-span-12"
            >
              <div className="grid gap-4 md:grid-cols-2">
                {(
                  [
                    "merchant_id",
                    "username",
                    "password",
                    "callback_secret",
                  ] as const
                ).map((field) => (
                  <Field
                    key={field}
                    id={`${environment}-${field}`}
                    label={t(`payments.credentials.${field}`)}
                  >
                    <Input
                      name={`${environment}.${field}`}
                      type="password"
                      autoComplete="new-password"
                      defaultValue={
                        payment.payment_providers?.globadyme?.[environment]?.[
                          field
                        ] ?? ""
                      }
                      maxLength={2048}
                    />
                  </Field>
                ))}
              </div>
              <p className="text-sm text-muted-foreground">
                {t("payments.maskHint")}
              </p>
              <Field
                id={`${environment}-previous-secret`}
                label={t("payments.previousSecret")}
              >
                <Input
                  type="password"
                  readOnly
                  value={
                    payment.payment_providers?.globadyme?.[environment]
                      ?.previous_callback_secret ?? ""
                  }
                />
              </Field>
              <p className="text-sm text-muted-foreground">
                {t("payments.rotationHint")}
              </p>
              {payment.payment_providers?.globadyme?.[environment]
                ?.previous_callback_secret ? (
                <Button
                  type="button"
                  variant="outline"
                  className="self-start"
                  onClick={() => {
                    setRetire(environment)
                    retirement.reset()
                  }}
                >
                  {t("payments.retire")}
                </Button>
              ) : null}
            </Tile>
          ))}
          <ApiError error={save.error} fallback={t("common.error")} />
          <Button
            type="submit"
            className="self-start"
            disabled={save.isPending || settings.isFetching}
          >
            {t("invoices.save")}
          </Button>
        </form>
      )}
      {retire ? (
        <Tile
          id="retire-secret"
          title={t("payments.retire")}
          span="col-span-12"
        >
          <form
            className="flex flex-col gap-4"
            onSubmit={(event) => {
              event.preventDefault()
              const form = new FormData(event.currentTarget)
              retirement.mutate({
                reason: String(form.get("reason")).trim(),
                evidence: String(form.get("evidence")).trim(),
              })
            }}
          >
            <Notice variant="warning" title={t("payments.retireHint")} />
            <Field id="retire-reason" label={t("payments.reason")}>
              <Input name="reason" maxLength={1000} required />
            </Field>
            <Field id="retire-evidence" label={t("payments.evidence")}>
              <Textarea name="evidence" maxLength={2000} required />
            </Field>
            <ApiError error={retirement.error} fallback={t("common.error")} />
            <div className="flex flex-wrap gap-2">
              <Button type="submit" disabled={retirement.isPending}>
                {t("payments.retire")}
              </Button>
              <Button
                type="button"
                variant="outline"
                onClick={() => setRetire(null)}
              >
                {t("actions.cancel")}
              </Button>
            </div>
          </form>
        </Tile>
      ) : null}
    </>
  )
}
