"use client"

import * as React from "react"
import { useParams, useRouter, useSearchParams } from "next/navigation"
import { useTranslations } from "next-intl"

import {
  useIntegrationWrite,
  useMappings,
} from "@genesy/api-client/business-approvals"
import {
  integrationMappingsDelete,
  integrationMappingsUpsert,
} from "@genesy/api-client/endpoints/integrations"
import { fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import {
  CODES,
  errorText,
  label,
} from "@genesy/ui/components/business-approvals/labels"
import type { MappingView } from "@genesy/ui/components/business-approvals/types"
import { Field, selectClass } from "@/components/field"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { Done, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"

/** Odoo ↔ NetSuite record matches for one workflow; `?entity=` opens on one kind (blocked reasons link here). */
export default function MappingsPage() {
  const t = useTranslations("businessApprovals")
  const { pair } = useParams<{ pair: string }>()
  const params = useSearchParams()
  const router = useRouter()
  const entity = params.get("entity") ?? ""
  const manage = useCan("integration.manage")
  const list = useMappings<MappingView>(pair, entity || undefined)
  const write = useIntegrationWrite()
  const errors = fieldErrors(write.error)
  const done = useDone()
  const form = React.useRef<HTMLFormElement>(null)
  const [removing, setRemoving] = React.useState<{
    m: MappingView
    open: boolean
  }>()
  const name = (m: MappingView) => m.source_label ?? m.source_id

  return (
    <>
      <PageHead
        back={{ href: "/settings/integrations", label: t("mappings.back") }}
        title={t("mappings.title")}
        description={t("mappings.intro")}
      />
      <Done message={done.message} />
      <Field id="mapping-filter" label={t("mappings.entity")}>
        <select
          className={`${selectClass} max-w-64`}
          value={entity}
          onChange={(e) =>
            router.replace(
              e.target.value
                ? `/settings/integrations/${pair}/mappings?entity=${e.target.value}`
                : `/settings/integrations/${pair}/mappings`
            )
          }
        >
          <option value="">{t("mappings.allEntities")}</option>
          {CODES.entity.map((e) => (
            <option key={e} value={e}>
              {label(t, "entity", e)}
            </option>
          ))}
        </select>
      </Field>

      {manage ? (
        <form
          ref={form}
          aria-label={t("mappings.add")}
          className="grid gap-3 rounded-xl border p-4 md:grid-cols-2"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const v = (k: string) => String(f.get(k) ?? "").trim()
            done.clear()
            write.mutate(
              (ws) =>
                integrationMappingsUpsert(ws, pair, {
                  entity: v("entity"),
                  source_id: v("source_id"),
                  target_id: v("target_id"),
                  ...(v("source_label")
                    ? { source_label: v("source_label") }
                    : {}),
                  ...(v("target_label")
                    ? { target_label: v("target_label") }
                    : {}),
                }),
              {
                onSuccess: () => {
                  done.show(t("mappings.added"))
                  form.current?.reset()
                },
              }
            )
          }}
        >
          <Field
            id="m-entity"
            label={t("mappings.entity")}
            errors={errors.entity}
          >
            <select
              name="entity"
              className={selectClass}
              defaultValue={entity || "vendor"}
              key={entity}
            >
              {CODES.entity.map((e) => (
                <option key={e} value={e}>
                  {label(t, "entity", e)}
                </option>
              ))}
            </select>
          </Field>
          <Field
            id="m-source"
            label={t("mappings.sourceId")}
            errors={errors.source_id}
          >
            <Input name="source_id" required />
          </Field>
          <Field
            id="m-target"
            label={t("mappings.targetId")}
            errors={errors.target_id}
          >
            <Input name="target_id" required />
          </Field>
          <Field id="m-source-label" label={t("mappings.sourceLabel")}>
            <Input name="source_label" />
          </Field>
          <Field id="m-target-label" label={t("mappings.targetLabel")}>
            <Input name="target_label" />
          </Field>
          {write.error && !Object.keys(errors).length ? (
            <Notice variant="error" title={errorText(t, write.error)} />
          ) : null}
          <div className="md:col-span-2">
            <Button
              type="submit"
              className="max-md:min-h-11"
              disabled={write.isPending}
            >
              {t("mappings.add")}
            </Button>
          </div>
        </form>
      ) : null}

      <ListCard>
        {list.isPending ? (
          <ListSkeleton columns={[{ label: "" }, { label: "" }]} />
        ) : list.isError ? (
          <CardState>
            <LoadError
              error={list.error}
              onRetry={() => void list.refetch()}
              backHref="/settings/integrations"
            />
          </CardState>
        ) : list.data.data.length === 0 ? (
          <CardState>
            <EmptyState
              title={t("mappings.empty")}
              description={t("mappings.emptyText")}
            />
          </CardState>
        ) : (
          <ul className="divide-y">
            {list.data.data.map((m) => (
              <li
                key={m.id}
                className="flex flex-wrap items-center justify-between gap-2 p-4"
              >
                <span className="flex flex-col">
                  <span className="text-sm text-muted-foreground">
                    {label(t, "entity", m.entity)}
                  </span>
                  <span>
                    {name(m)} → {m.target_label ?? m.target_id}
                  </span>
                </span>
                {manage ? (
                  <Button
                    variant="outline"
                    className="max-md:min-h-11"
                    aria-label={`${t("mappings.delete")}: ${name(m)}`}
                    onClick={() => setRemoving({ m, open: true })}
                  >
                    {t("mappings.delete")}
                  </Button>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </ListCard>
      <ConfirmDialog
        open={removing?.open ?? false}
        onOpenChange={(open) => setRemoving((r) => r && { ...r, open })}
        title={t("mappings.deleteTitle")}
        description={t("mappings.deleteText", {
          source: removing ? name(removing.m) : "",
        })}
        confirm={t("mappings.delete")}
        cancel={t("card.cancel")}
        disabled={write.isPending}
        onConfirm={() => {
          if (!removing) return
          done.clear()
          write.mutate(
            (ws) => integrationMappingsDelete(ws, pair, removing.m.id),
            { onSuccess: () => done.show(t("mappings.deleted")) }
          )
        }}
      />
    </>
  )
}
