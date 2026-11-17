"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import { useTranslations } from "next-intl"
import { Trash2 } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import { useProductCategoriesList } from "@genesy/api-client/endpoints/product-categories"
import {
  getTaxProfilesGetQueryKey,
  getTaxProfilesListQueryKey,
  useTaxProfilesCreate,
  useTaxProfilesUpdate,
} from "@genesy/api-client/endpoints/tax-profiles"
import type {
  TaxProfileCreate,
  TaxProfileResource,
} from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { Checkbox } from "@genesy/ui/components/checkbox"
import { Input } from "@genesy/ui/components/input"
import { Choices, Field, selectClass } from "@/components/field"
import { Capped } from "@/components/states"
import { useIdempotencyKey } from "@/lib/idempotency"

import { isStale, today } from "../../purchases/shared"
import { useReasons, useWord } from "./shared"

type Component = {
  key: string
  code: string
  name: string
  rate: string
  recoverable: boolean
}
type Exemption = {
  key: string
  product_category_id: string
  reason: string
  evidence_ref: string
}

const blankComponent = (): Component => ({
  key: crypto.randomUUID(),
  code: "",
  name: "",
  rate: "",
  recoverable: true,
})
const blankExemption = (): Exemption => ({
  key: crypto.randomUUID(),
  product_category_id: "",
  reason: "",
  evidence_ref: "",
})

/**
 * A tax profile draft (Form card, as the purchase order form): create, a correction of a
 * verified profile (`corrects`, prefilled from it), or a change to a draft (`profile`).
 */
export function TaxForm({
  profile,
  corrects,
}: {
  profile?: TaxProfileResource
  corrects?: TaxProfileResource
}) {
  const t = useTranslations()
  const router = useRouter()
  const word = useWord()
  const reasons = useReasons()
  const queryClient = useQueryClient()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  // shortcut: the first 100 categories; page these when a workspace has more.
  const categories = useProductCategoriesList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const key = useIdempotencyKey()
  const from = profile ?? corrects
  const [components, setComponents] = React.useState<Component[]>(() =>
    from?.components.length
      ? from.components.map((c) => ({ key: crypto.randomUUID(), ...c }))
      : [blankComponent()]
  )
  const [exemptions, setExemptions] = React.useState<Exemption[]>(() =>
    (from?.exemptions ?? [])
      .filter((e) => e.product_category_id)
      .map((e) => ({
        key: crypto.randomUUID(),
        product_category_id: e.product_category_id ?? "",
        reason: e.reason,
        evidence_ref: e.evidence_ref,
      }))
  )
  const done = (id: string, message: "created" | "saved") => {
    for (const queryKey of [
      getTaxProfilesListQueryKey(workspaceId),
      getTaxProfilesGetQueryKey(workspaceId, id),
    ])
      void queryClient.invalidateQueries({ queryKey })
    router.push(`/settings/taxes/${id}?done=${message}`)
  }
  const create = useTaxProfilesCreate({
    mutation: {
      onSuccess: (r) => done(r.data.id, "created"),
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const update = useTaxProfilesUpdate({
    mutation: {
      onSuccess: (r) => done(r.data.id, "saved"),
      onError: (e) => {
        if (isStale(e) && profile)
          void queryClient.invalidateQueries({
            queryKey: getTaxProfilesGetQueryKey(workspaceId, profile.id),
          })
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const mutation = profile ? update : create
  const errors = fieldErrors(mutation.error)
  const err = (field: string) => errors[`payload.${field}`]
  const setComponent = (k: string, patch: Partial<Component>) =>
    setComponents((cs) => cs.map((c) => (c.key === k ? { ...c, ...patch } : c)))
  const setExemption = (k: string, patch: Partial<Exemption>) =>
    setExemptions((es) => es.map((e) => (e.key === k ? { ...e, ...patch } : e)))
  const categoryOptions = (categories.data?.data ?? []).map((c) => ({
    value: c.product_category_id,
    label: c.name,
  }))

  return (
    <form
      className="flex flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault()
        const f = new FormData(e.currentTarget)
        const text = (k: string) => String(f.get(k) ?? "").trim()
        const payload: TaxProfileCreate = {
          country: text("country").toUpperCase(),
          jurisdiction_scope: text("jurisdiction_scope").toUpperCase() || null,
          effective_from: text("effective_from"),
          effective_to: text("effective_to") || null,
          transaction_types: f.getAll("transaction_types").map(String),
          product_categories: f.getAll("product_categories").map(String),
          tax_point: text("tax_point"),
          evidence_ref: text("evidence_ref") || null,
          components: components.map((c) => ({
            code: c.code.trim(),
            name: c.name.trim(),
            kind: "percentage",
            rate: c.rate.trim(),
            recoverable: c.recoverable,
          })),
          exemptions: exemptions.map((x) => ({
            product_category_id: x.product_category_id,
            reason: x.reason.trim(),
            evidence_ref: x.evidence_ref.trim(),
          })),
        }
        const client_ref = key.request.headers["Idempotency-Key"]
        if (profile)
          update.mutate({
            workspace: workspaceId,
            taxProfile: profile.id,
            data: { client_ref, expected_version: profile.version, payload },
          })
        else
          create.mutate({
            workspace: workspaceId,
            data: {
              client_ref,
              payload: { ...payload, corrects_id: corrects?.id ?? null },
            },
          })
      }}
    >
      <Card className="gap-4 px-6 py-6 max-md:p-4">
        <div className="grid gap-4 md:grid-cols-2">
          <Field
            id="tx-country"
            label={t("taxes.country")}
            errors={err("country")}
          >
            <Input
              name="country"
              required
              minLength={2}
              maxLength={2}
              autoCapitalize="characters"
              defaultValue={from?.country ?? ""}
            />
          </Field>
          <Field
            id="tx-scope"
            label={t("taxes.scope")}
            errors={err("jurisdiction_scope")}
          >
            <Input
              name="jurisdiction_scope"
              maxLength={6}
              defaultValue={from?.jurisdiction_scope ?? ""}
            />
          </Field>
          <Field
            id="tx-from"
            label={t("taxes.from")}
            errors={err("effective_from")}
          >
            <Input
              type="date"
              name="effective_from"
              required
              defaultValue={from?.effective_from ?? today()}
            />
          </Field>
          <Field id="tx-to" label={t("taxes.to")} errors={err("effective_to")}>
            <Input
              type="date"
              name="effective_to"
              defaultValue={from?.effective_to ?? ""}
            />
          </Field>
          <Field
            id="tx-point"
            label={t("taxes.taxPoint")}
            errors={err("tax_point")}
          >
            <select
              name="tax_point"
              className={selectClass}
              defaultValue={from?.tax_point ?? "invoice"}
            >
              {["invoice", "supply"].map((p) => (
                <option key={p} value={p}>
                  {word("taxPoints", p)}
                </option>
              ))}
            </select>
          </Field>
          <Field
            id="tx-evidence"
            label={t("taxes.evidence")}
            errors={err("evidence_ref")}
          >
            <Input
              name="evidence_ref"
              maxLength={255}
              defaultValue={from?.evidence_ref ?? ""}
            />
          </Field>
        </div>
        <p className="text-[13px] text-muted-foreground">
          {t("taxes.scopeHint")}
        </p>
        <div className="grid gap-4 md:grid-cols-2">
          <Choices
            name="transaction_types"
            legend={t("taxes.appliesTo")}
            options={["sale", "purchase"].map((x) => ({
              value: x,
              label: word("transactionTypes", x),
            }))}
            defaultValue={from?.transaction_types ?? ["sale"]}
            errors={err("transaction_types")}
          />
          <div className="flex flex-col gap-2">
            <Capped meta={categories.data?.meta} />
            <Choices
              key={categories.data ? "loaded" : "loading"}
              name="product_categories"
              legend={t("taxes.categories")}
              options={categoryOptions}
              defaultValue={from?.product_categories ?? []}
              errors={err("product_categories")}
            />
            <p className="text-[13px] text-muted-foreground">
              {t("taxes.categoriesHint")}
            </p>
          </div>
        </div>
      </Card>

      <Card
        className="gap-4 px-6 py-6 max-md:p-4"
        role="region"
        aria-labelledby="tx-components"
      >
        <h2
          id="tx-components"
          className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
        >
          {t("taxes.components")}
        </h2>
        <p className="text-[13px] text-muted-foreground">
          {t("taxes.componentsHint")}
        </p>
        {err("components")?.length ? (
          <p className="text-sm text-destructive">
            {err("components")!.join(" ")}
          </p>
        ) : null}
        {components.map((c, i) => (
          <fieldset
            key={c.key}
            className="grid gap-3 border-t border-(--hair) pt-4 first-of-type:border-0 first-of-type:pt-0 md:grid-cols-[1fr_2fr_1fr_auto_auto] md:items-end"
          >
            <legend className="sr-only">
              {t("taxes.component", { n: i + 1 })}
            </legend>
            <Field
              id={`tx-${c.key}-code`}
              label={t("taxes.code")}
              errors={err(`components.${i}.code`)}
            >
              <Input
                required
                maxLength={32}
                value={c.code}
                onChange={(e) => setComponent(c.key, { code: e.target.value })}
              />
            </Field>
            <Field
              id={`tx-${c.key}-name`}
              label={t("taxes.name")}
              errors={err(`components.${i}.name`)}
            >
              <Input
                required
                maxLength={255}
                value={c.name}
                onChange={(e) => setComponent(c.key, { name: e.target.value })}
              />
            </Field>
            <Field
              id={`tx-${c.key}-rate`}
              label={t("taxes.rate")}
              errors={err(`components.${i}.rate`) ?? err(`components.${i}`)}
            >
              <Input
                required
                inputMode="decimal"
                value={c.rate}
                onChange={(e) => setComponent(c.key, { rate: e.target.value })}
              />
            </Field>
            <label className="flex h-11 items-center gap-2 text-sm">
              <Checkbox
                checked={c.recoverable}
                onCheckedChange={(v) =>
                  setComponent(c.key, { recoverable: Boolean(v) })
                }
              />
              {t("taxes.recoverable")}
            </label>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="max-md:justify-self-end"
              aria-label={t("taxes.removeComponent", { n: i + 1 })}
              disabled={components.length === 1}
              onClick={() =>
                setComponents((cs) => cs.filter((x) => x.key !== c.key))
              }
            >
              <Trash2 className="size-4" />
            </Button>
          </fieldset>
        ))}
        <Button
          type="button"
          variant="outline"
          className="self-start"
          disabled={components.length >= 10}
          onClick={() => setComponents((cs) => [...cs, blankComponent()])}
        >
          {t("taxes.addComponent")}
        </Button>
      </Card>

      <Card
        className="gap-4 px-6 py-6 max-md:p-4"
        role="region"
        aria-labelledby="tx-exemptions"
      >
        <h2
          id="tx-exemptions"
          className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
        >
          {t("taxes.exemptions")}
        </h2>
        <p className="text-[13px] text-muted-foreground">
          {t("taxes.exemptionsHint")}
        </p>
        {err("exemptions")?.length ? (
          <p className="text-sm text-destructive">
            {err("exemptions")!.join(" ")}
          </p>
        ) : null}
        {exemptions.map((x, i) => (
          <fieldset
            key={x.key}
            className="grid gap-3 border-t border-(--hair) pt-4 first-of-type:border-0 first-of-type:pt-0 md:grid-cols-[1.5fr_2fr_1fr_auto] md:items-end"
          >
            <legend className="sr-only">
              {t("taxes.exemption", { n: i + 1 })}
            </legend>
            <Field
              id={`tx-${x.key}-cat`}
              label={t("taxes.category")}
              errors={
                err(`exemptions.${i}.product_category_id`) ??
                err(`exemptions.${i}`)
              }
            >
              <select
                className={selectClass}
                required
                value={x.product_category_id}
                onChange={(e) =>
                  setExemption(x.key, { product_category_id: e.target.value })
                }
              >
                <option value="" disabled>
                  {t("taxes.chooseCategory")}
                </option>
                {categoryOptions.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </Field>
            <Field
              id={`tx-${x.key}-reason`}
              label={t("taxes.reason")}
              errors={err(`exemptions.${i}.reason`)}
            >
              <Input
                required
                maxLength={1000}
                value={x.reason}
                onChange={(e) =>
                  setExemption(x.key, { reason: e.target.value })
                }
              />
            </Field>
            <Field
              id={`tx-${x.key}-evidence`}
              label={t("taxes.evidence")}
              errors={err(`exemptions.${i}.evidence_ref`)}
            >
              <Input
                required
                maxLength={255}
                value={x.evidence_ref}
                onChange={(e) =>
                  setExemption(x.key, { evidence_ref: e.target.value })
                }
              />
            </Field>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="max-md:justify-self-end"
              aria-label={t("taxes.removeExemption", { n: i + 1 })}
              onClick={() =>
                setExemptions((es) => es.filter((e) => e.key !== x.key))
              }
            >
              <Trash2 className="size-4" />
            </Button>
          </fieldset>
        ))}
        <Button
          type="button"
          variant="outline"
          className="self-start"
          disabled={exemptions.length >= 100 || categoryOptions.length === 0}
          onClick={() => setExemptions((es) => [...es, blankExemption()])}
        >
          {t("taxes.addExemption")}
        </Button>
      </Card>

      <ApiError
        error={mutation.error}
        fallback={t("common.error")}
        codes={reasons}
      />
      <Button
        type="submit"
        size="lg"
        className="self-end max-md:w-full"
        disabled={mutation.isPending}
      >
        {t("taxes.saveDraft")}
      </Button>
    </form>
  )
}
