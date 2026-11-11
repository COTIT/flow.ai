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
type SettingsResponse = {
  data: unknown
  meta?: {
    globadyme_environment?: "sandbox" | "production" | null
    production_origin_verified?: boolean
  }
}
const CREDENTIAL_FIELDS = [
  "merchant_id",
  "username",
  "password",
  "callback_secret",
] as const

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
  const settings = useQuery<SettingsResponse>({
    queryKey,
    queryFn: ({ signal }) =>
      settingsGet(workspace, { type: "integrations" }, undefined, signal),
    retry: false,
  })
  const [retire, setRetire] = React.useState<"sandbox" | "production" | null>(
    null
  )
  const [draftProvider, setDraftProvider] = React.useState<string>()
  const save = useMutation({
    mutationFn: (integrations: Integrations) =>
      settingsUpdate(workspace, { integrations }),
    onMutate: done.clear,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey })
      setDraftProvider(undefined)
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
  const selectedProvider =
    draftProvider ??
    (payment.active_provider === "globadyme" ? "globadyme" : "")
  const reportedRealm = settings.data?.meta?.globadyme_environment
  const currentRealm =
    reportedRealm === "sandbox" || reportedRealm === "production"
      ? reportedRealm
      : null
  const realmLabel = currentRealm
    ? t(
        currentRealm === "sandbox"
          ? "payments.sandboxStaging"
          : "payments.production"
      )
    : t("payments.environmentUnavailable")
  const savedSlot = currentRealm
    ? payment.payment_providers?.globadyme?.[currentRealm]
    : undefined
  const missingFields = CREDENTIAL_FIELDS.filter((field) => {
    const value = savedSlot?.[field]
    return !(field === "merchant_id" || field === "username"
      ? value?.trim()
      : value)
  })
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
            const provider = String(form.get("provider")) || null
            const nextPayment: PaymentSettings = {
              ...payment,
              active_provider: provider,
              enabled: form.get("enabled") === "on",
            }
            if (provider === "globadyme") {
              if (!currentRealm) return
              const providers = { ...payment.payment_providers }
              const globadyme = { ...providers.globadyme }
              const slot = { ...globadyme[currentRealm] }
              for (const field of CREDENTIAL_FIELDS) {
                const value = String(form.get(`${currentRealm}.${field}`) ?? "")
                slot[field] =
                  (field === "merchant_id" || field === "username"
                    ? value.trim()
                    : value) || null
              }
              globadyme[currentRealm] = slot
              providers.globadyme = globadyme
              nextPayment.environment = currentRealm
              nextPayment.payment_providers = providers
            }
            save.mutate({
              ...integrations,
              payment: nextPayment,
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
                  value={selectedProvider}
                  onChange={(event) => setDraftProvider(event.target.value)}
                >
                  <option value="">{t("payments.legacy")}</option>
                  <option value="globadyme">Globadyme</option>
                </select>
              </Field>
              <Field id="payment-environment" label={t("payments.environment")}>
                <Input value={realmLabel} readOnly />
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
            <p className="text-sm text-muted-foreground">
              {t("payments.deploymentRealmHint")}
            </p>
            {!currentRealm && selectedProvider === "globadyme" ? (
              <Notice variant="warning" title={t("payments.setupUnavailable")}>
                {t("payments.setupUnavailableHint")}
              </Notice>
            ) : null}
            {selectedProvider === "globadyme" &&
            currentRealm === "production" &&
            settings.data?.meta?.production_origin_verified !== true ? (
              <Notice
                variant="warning"
                title={t("payments.productionBlocked")}
              />
            ) : null}
            {currentRealm &&
            payment.active_provider === "globadyme" &&
            missingFields.length ? (
              <Notice variant="warning" title={t("payments.setupIncomplete")}>
                <p>
                  {t("payments.savedSetupHint", {
                    environment: realmLabel,
                  })}
                </p>
                <ul className="mt-2 list-disc pl-5">
                  {missingFields.map((field) => (
                    <li key={field}>{t(`payments.credentials.${field}`)}</li>
                  ))}
                </ul>
              </Notice>
            ) : null}
            <p className="text-sm text-muted-foreground">
              {t("payments.identityHint")}
            </p>
          </Tile>
          {currentRealm ? (
            <Tile
              key={currentRealm}
              id={`payment-${currentRealm}`}
              title={`Globadyme · ${realmLabel}`}
              span="col-span-12"
            >
              <fieldset className="flex flex-col gap-4">
                <legend className="sr-only">{`Globadyme · ${realmLabel}`}</legend>
                <div className="grid gap-4 md:grid-cols-2">
                  {CREDENTIAL_FIELDS.map((field) => (
                    <Field
                      key={field}
                      id={`${currentRealm}-${field}`}
                      label={t(`payments.credentials.${field}`)}
                    >
                      <Input
                        name={`${currentRealm}.${field}`}
                        type="password"
                        autoComplete="new-password"
                        defaultValue={savedSlot?.[field] ?? ""}
                        maxLength={2048}
                      />
                    </Field>
                  ))}
                </div>
                <p className="text-sm text-muted-foreground">
                  {t("payments.maskHint")}
                </p>
                <Field
                  id={`${currentRealm}-previous-secret`}
                  label={t("payments.previousSecret")}
                >
                  <Input
                    type="password"
                    readOnly
                    value={savedSlot?.previous_callback_secret ?? ""}
                  />
                </Field>
                <p className="text-sm text-muted-foreground">
                  {t("payments.rotationHint")}
                </p>
                {savedSlot?.previous_callback_secret ? (
                  <Button
                    type="button"
                    variant="outline"
                    className="self-start"
                    onClick={() => {
                      setRetire(currentRealm)
                      retirement.reset()
                    }}
                  >
                    {t("payments.retire")}
                  </Button>
                ) : null}
              </fieldset>
            </Tile>
          ) : null}
          <ApiError error={save.error} fallback={t("common.error")} />
          <Button
            type="submit"
            className="self-start"
            disabled={
              (selectedProvider === "globadyme" && !currentRealm) ||
              save.isPending ||
              settings.isFetching
            }
          >
            {t("invoices.save")}
          </Button>
        </form>
      )}
      {retire && retire === currentRealm ? (
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
