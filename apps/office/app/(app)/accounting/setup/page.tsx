"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  getAccountingSetupGetQueryKey,
  useAccountingAccountsList,
  useAccountingSetupGet,
  useAccountingSetupUpdate,
} from "@genesy/api-client/endpoints/accounting"
import { useCurrenciesList } from "@genesy/api-client/endpoints/currencies"
import type { SetupResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { Notice } from "@genesy/ui/components/notice"
import { formatDateTime } from "@genesy/ui/lib/format"
import { DetailSkeleton } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"
import { useIdempotencyKey } from "@/lib/idempotency"

import { AccountingTabs, isStale, roles, useReasons, useWord } from "../shared"

/** The book's setup (Form card): currency, timezone, role → account. Read-only once active. */
export default function SetupPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const setup = useAccountingSetupGet(workspaceId, {
    query: { enabled: Boolean(workspaceId) },
  })
  // Kept here: the form remounts on each saved version, its outcome must survive that.
  const done = useDone()
  return (
    <>
      <PageHead
        eyebrow={t("shell.accounting")}
        title={t("accounting.setup.title")}
        tabs={<AccountingTabs />}
      />
      {setup.isPending ? (
        <DetailSkeleton />
      ) : setup.isError ? (
        <LoadError error={setup.error} onRetry={() => void setup.refetch()} />
      ) : (
        // Remount on a new version so the form starts from the saved values.
        <SetupForm
          key={setup.data.data.version}
          setup={setup.data.data}
          done={done}
        />
      )}
    </>
  )
}

function SetupForm({
  setup,
  done,
}: {
  setup: SetupResource
  done: ReturnType<typeof useDone>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = useWord()
  const reasons = useReasons()
  const queryClient = useQueryClient()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const mayConfigure = useCan("accounting.configure")
  const editable = mayConfigure && setup.allowed_actions.includes("update")
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  // shortcut: first 100 currencies and accounts; page these when a book holds more.
  const currencies = useCurrenciesList(workspaceId, { page_size: 100 }, enabled)
  const accounts = useAccountingAccountsList(
    workspaceId,
    { page_size: "100" },
    enabled
  )
  const key = useIdempotencyKey()
  const refresh = () =>
    void queryClient.invalidateQueries({
      queryKey: getAccountingSetupGetQueryKey(workspaceId),
    })
  const save = useAccountingSetupUpdate({
    mutation: {
      onSuccess: () => {
        done.show(t("done.accounting.setupSaved"))
        refresh()
      },
      onError: (e) => {
        if (isStale(e)) refresh()
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const errors = fieldErrors(save.error)
  const mappings = setup.account_mappings as Record<string, string | undefined>
  const zones = React.useMemo(() => {
    const all = Intl.supportedValuesOf("timeZone")
    return setup.timezone && !all.includes(setup.timezone)
      ? [setup.timezone, ...all]
      : all
  }, [setup.timezone])
  const active = (accounts.data?.data ?? []).filter((a) => a.active)

  return (
    <form
      className="flex flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault()
        done.clear()
        const f = new FormData(e.currentTarget)
        const text = (k: string) => String(f.get(k) ?? "")
        const account_mappings = Object.fromEntries(
          roles.map((r) => [r, text(`role.${r}`)]).filter(([, id]) => id)
        )
        save.mutate({
          workspace: workspaceId,
          data: {
            client_ref: key.request.headers["Idempotency-Key"],
            expected_version: setup.version,
            payload: {
              functional_currency_id:
                text("functional_currency_id") || undefined,
              timezone: text("timezone") || undefined,
              account_mappings,
            },
          },
        })
      }}
    >
      <Done message={done.message} />
      {setup.state === "active" ? (
        <Notice
          variant="info"
          title={t("accounting.setup.activeInfo", {
            at: setup.activated_at
              ? formatDateTime(setup.activated_at, locale)
              : "",
          })}
        />
      ) : (
        <Notice variant="info" title={t("accounting.setup.setupInfo")} />
      )}
      {!editable && setup.state !== "active" ? (
        <Notice variant="info" title={t("accounting.setup.readOnly")} />
      ) : null}
      {setup.missing_roles.length ? (
        <Notice
          variant="warning"
          title={t("accounting.setup.missing", {
            count: setup.missing_roles.length,
          })}
        >
          {setup.missing_roles.map((r) => word("roles", r)).join(", ")}
        </Notice>
      ) : setup.configured ? (
        <Notice
          variant="success"
          live={false}
          title={t("accounting.setup.configured")}
        />
      ) : null}

      <Card className="gap-4 px-6 py-6 max-md:p-4">
        <div className="flex items-center justify-between gap-3">
          <h2 className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase">
            {t("accounting.setup.book")}
          </h2>
          <StatusBadge
            status={setup.state}
            label={word("bookStates", setup.state)}
            tone={setup.state === "active" ? "teal" : "gold"}
          />
        </div>
        <fieldset disabled={!editable} className="grid gap-4 md:grid-cols-2">
          <Field
            id="setup-currency"
            label={t("accounting.setup.currency")}
            errors={errors["payload.functional_currency_id"]}
          >
            <select
              key={currencies.data ? "loaded" : "loading"}
              name="functional_currency_id"
              className={selectClass}
              defaultValue={setup.functional_currency?.id ?? ""}
            >
              <option value="">{t("accounting.setup.chooseCurrency")}</option>
              {setup.functional_currency &&
              !currencies.data?.data.some(
                (c) => c.id === setup.functional_currency?.id
              ) ? (
                <option value={setup.functional_currency.id}>
                  {setup.functional_currency.code}
                </option>
              ) : null}
              {currencies.data?.data.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.asset_type}
                </option>
              ))}
            </select>
          </Field>
          <Field
            id="setup-timezone"
            label={t("accounting.setup.timezone")}
            errors={errors["payload.timezone"]}
          >
            <select
              name="timezone"
              className={selectClass}
              defaultValue={setup.timezone ?? ""}
            >
              <option value="">{t("accounting.setup.chooseTimezone")}</option>
              {zones.map((z) => (
                <option key={z} value={z}>
                  {z}
                </option>
              ))}
            </select>
          </Field>
        </fieldset>
      </Card>

      <Card
        className="gap-4 px-6 py-6 max-md:p-4"
        role="region"
        aria-labelledby="setup-roles"
      >
        <h2
          id="setup-roles"
          className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
        >
          {t("accounting.setup.roles")}
        </h2>
        <p className="text-[13px] text-muted-foreground">
          {t("accounting.setup.rolesHint")}
        </p>
        {errors["payload.account_mappings"]?.length ? (
          <p className="text-sm text-destructive">
            {errors["payload.account_mappings"].join(" ")}
          </p>
        ) : null}
        <fieldset disabled={!editable} className="grid gap-4 md:grid-cols-2">
          {roles.map((r) => (
            <Field
              key={`${r}-${accounts.data ? "loaded" : "loading"}`}
              id={`setup-role-${r}`}
              label={word("roles", r)}
              errors={errors[`payload.account_mappings.${r}`]}
            >
              <select
                name={`role.${r}`}
                className={selectClass}
                defaultValue={mappings[r] ?? ""}
              >
                <option value="">{t("accounting.setup.unmapped")}</option>
                {active.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.code} {a.name}
                  </option>
                ))}
              </select>
            </Field>
          ))}
        </fieldset>
      </Card>

      <ApiError
        error={save.error}
        fallback={t("common.error")}
        codes={reasons}
      />
      {editable ? (
        <Button
          type="submit"
          size="lg"
          className="self-end max-md:w-full"
          disabled={save.isPending}
        >
          {t("accounting.setup.save")}
        </Button>
      ) : null}
    </form>
  )
}
