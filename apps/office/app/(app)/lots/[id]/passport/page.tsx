"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import { useFilesList } from "@genesy/api-client/endpoints/files"
import {
  getLotsPassportGetQueryKey,
  useLotsGet,
  useLotsPassportGet,
  useLotsPassportPublish,
  useLotsPassportSave,
  useLotsPassportUnpublish,
} from "@genesy/api-client/endpoints/lots"
import { useProductsGet } from "@genesy/api-client/endpoints/products"
import type {
  Block,
  FileResource,
  LotResource,
  Passport,
  PassportResource,
} from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { Input } from "@genesy/ui/components/input"
import { PassportView } from "@genesy/ui/components/passport-view"
import { Textarea } from "@genesy/ui/components/textarea"
import { Field, selectClass } from "@/components/field"
import { LoadError } from "@/components/states"

import { ListEditor } from "./list-editor"
import { bpToPercent, percentToBp, toPublic } from "./to-public"

export default function PassportPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const lot = useLotsGet(workspaceId, id, enabled)
  const passport = useLotsPassportGet(workspaceId, id, enabled)
  // shortcut: pickers and preview see the first 100 files; add file search when workspaces hold more.
  const files = useFilesList(workspaceId, { page_size: 100 }, enabled)

  if (lot.isPending || passport.isPending)
    return (
      <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
    )
  if (lot.isError || passport.isError)
    return (
      <LoadError
        error={lot.error ?? passport.error}
        onRetry={() => {
          void lot.refetch()
          void passport.refetch()
        }}
      />
    )
  return (
    <Editor
      // A fresh copy of the server's draft after every save, publish or unpublish.
      key={passport.dataUpdatedAt}
      lot={lot.data.data}
      resource={passport.data.data}
      files={files.data?.data ?? []}
    />
  )
}

function Editor({
  lot,
  resource,
  files,
}: {
  lot: LotResource
  resource: PassportResource
  files: FileResource[]
}) {
  const t = useTranslations()
  const tp = useTranslations("passport")
  const tb = useTranslations("batch")
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const product = useProductsGet(workspaceId, lot.product_id)
  const [draft, setDraft] = React.useState<Passport>(resource.draft)
  const set = (patch: Partial<Passport>) =>
    setDraft((d) => ({ ...d, ...patch }))
  // Typed text per coordinate; the map point exists only once both are filled in.
  const [coords, setCoords] = React.useState({
    lat: draft.map ? String(draft.map.lat) : "",
    lng: draft.map ? String(draft.map.lng) : "",
  })
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getLotsPassportGetQueryKey(workspaceId, lot.id),
    })
  const vars = { workspace: workspaceId, lot: lot.id }
  const save = useLotsPassportSave({ mutation: { onSuccess: refresh } })
  const publish = useLotsPassportPublish({ mutation: { onSuccess: refresh } })
  const unpublish = useLotsPassportUnpublish({
    mutation: { onSuccess: refresh },
  })
  const errors = fieldErrors(save.error ?? publish.error)
  const unsaved = JSON.stringify(draft) !== JSON.stringify(resource.draft)
  const unpublished =
    JSON.stringify(resource.draft) !== JSON.stringify(resource.published)
  const shopUrl = process.env.NEXT_PUBLIC_SHOP_URL

  const fileSelect = (
    value: string | null | undefined,
    onChange: (id: string | null) => void,
    id: string
  ): React.ReactElement<React.ComponentProps<"select">> => (
    <select
      id={id}
      className={selectClass}
      value={value ?? ""}
      onChange={(e) => onChange(e.target.value || null)}
    >
      <option value="">{tp("noFile")}</option>
      {files.map((f) => (
        <option key={f.id} value={f.id}>
          {f.document_name}
        </option>
      ))}
    </select>
  )

  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <section className="flex min-w-0 flex-col gap-4">
        <div className="flex flex-wrap items-center gap-2">
          <h1 className="mr-auto heading-2 text-lg">
            {tp("title", { code: lot.code })}
          </h1>
          <Link
            className="text-sm underline-offset-4 hover:underline"
            href={`/lots/${lot.id}`}
          >
            {tp("backToBatch")}
          </Link>
        </div>
        <p role="status" className="text-sm text-muted-foreground">
          {resource.published_at
            ? tp("publishedState", {
                version: resource.version,
                date: new Date(resource.published_at).toLocaleString(locale),
              })
            : tp("notPublished")}
          {resource.published && unpublished ? ` ${tp("draftDiffers")}` : ""}
        </p>
        {resource.published && shopUrl ? (
          <a
            className="font-mono text-xs break-all underline-offset-4 hover:underline"
            href={`${shopUrl}/b/${lot.public_code}`}
            target="_blank"
            rel="noreferrer"
          >
            {`${shopUrl}/b/${lot.public_code}`}
          </a>
        ) : null}

        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault()
            save.mutate({ ...vars, data: draft })
          }}
        >
          <Field id="pp-title" label={tp("fields.title")} errors={errors.title}>
            <Input
              value={draft.title ?? ""}
              onChange={(e) => set({ title: e.target.value || null })}
            />
          </Field>
          <Field id="pp-intro" label={tp("fields.intro")} errors={errors.intro}>
            <Textarea
              value={draft.intro ?? ""}
              onChange={(e) => set({ intro: e.target.value || null })}
            />
          </Field>
          <Field
            id="pp-banner"
            label={tp("fields.banner")}
            errors={errors.banner_file_id}
          >
            {fileSelect(
              draft.banner_file_id,
              (v) => set({ banner_file_id: v }),
              "pp-banner"
            )}
          </Field>

          <fieldset className="grid gap-3 rounded-lg border border-border p-3 sm:grid-cols-3">
            <legend className="px-1 text-sm font-medium">
              {tp("fields.origin")}
            </legend>
            {(["country", "region", "place"] as const).map((k) => (
              <Field
                key={k}
                id={`pp-origin-${k}`}
                label={tp(`fields.${k}`)}
                errors={errors[`origin.${k}`]}
              >
                <Input
                  value={draft.origin?.[k] ?? ""}
                  onChange={(e) => {
                    const origin = {
                      country: "",
                      ...draft.origin,
                      [k]: e.target.value,
                    }
                    // All three empty means no origin at all.
                    set({
                      origin:
                        origin.country || origin.region || origin.place
                          ? {
                              ...origin,
                              region: origin.region || null,
                              place: origin.place || null,
                            }
                          : null,
                    })
                  }}
                />
              </Field>
            ))}
          </fieldset>

          <fieldset className="grid gap-3 rounded-lg border border-border p-3 sm:grid-cols-3">
            <legend className="px-1 text-sm font-medium">
              {tp("fields.map")}
            </legend>
            {(["lat", "lng"] as const).map((k) => (
              <Field
                key={k}
                id={`pp-map-${k}`}
                label={tp(`fields.${k}`)}
                errors={errors[`map.${k}`]}
              >
                <Input
                  type="number"
                  step="any"
                  value={coords[k]}
                  onChange={(e) => {
                    const next = { ...coords, [k]: e.target.value }
                    setCoords(next)
                    set({
                      map:
                        next.lat && next.lng
                          ? {
                              lat: Number(next.lat),
                              lng: Number(next.lng),
                              label: draft.map?.label ?? null,
                            }
                          : null,
                    })
                  }}
                />
              </Field>
            ))}
            <Field
              id="pp-map-label"
              label={tp("fields.mapLabel")}
              errors={errors["map.label"]}
            >
              <Input
                disabled={!draft.map}
                value={draft.map?.label ?? ""}
                onChange={(e) =>
                  draft.map &&
                  set({ map: { ...draft.map, label: e.target.value || null } })
                }
              />
            </Field>
          </fieldset>

          <ListEditor
            label={tp("fields.materials")}
            items={draft.materials ?? []}
            onChange={(materials) => set({ materials })}
            render={(m, setM, i) => (
              <div className="grid gap-2 sm:grid-cols-3">
                <Field
                  id={`pp-mat-${i}-name`}
                  label={tp("fields.name")}
                  errors={errors[`materials.${i}.name`]}
                >
                  <Input
                    value={m.name}
                    onChange={(e) => setM({ ...m, name: e.target.value })}
                  />
                </Field>
                <Field
                  id={`pp-mat-${i}-origin`}
                  label={tp("fields.materialOrigin")}
                  errors={errors[`materials.${i}.origin`]}
                >
                  <Input
                    value={m.origin ?? ""}
                    onChange={(e) =>
                      setM({ ...m, origin: e.target.value || null })
                    }
                  />
                </Field>
                <Field
                  id={`pp-mat-${i}-share`}
                  label={tp("fields.share")}
                  errors={errors[`materials.${i}.share_bp`]}
                >
                  <PercentInput
                    bp={m.share_bp}
                    onChange={(share_bp) => setM({ ...m, share_bp })}
                  />
                </Field>
              </div>
            )}
            add={
              <Button
                type="button"
                variant="outline"
                onClick={() =>
                  set({ materials: [...(draft.materials ?? []), { name: "" }] })
                }
              >
                {tp("addMaterial")}
              </Button>
            }
          />

          <ListEditor
            label={tp("fields.certificates")}
            items={draft.certificates ?? []}
            onChange={(certificates) => set({ certificates })}
            render={(c, setC, i) => (
              <div className="grid gap-2 sm:grid-cols-2">
                <Field
                  id={`pp-cert-${i}-name`}
                  label={tp("fields.name")}
                  errors={errors[`certificates.${i}.name`]}
                >
                  <Input
                    value={c.name}
                    onChange={(e) => setC({ ...c, name: e.target.value })}
                  />
                </Field>
                <Field
                  id={`pp-cert-${i}-issuer`}
                  label={tp("fields.issuer")}
                  errors={errors[`certificates.${i}.issuer`]}
                >
                  <Input
                    value={c.issuer ?? ""}
                    onChange={(e) =>
                      setC({ ...c, issuer: e.target.value || null })
                    }
                  />
                </Field>
                <Field
                  id={`pp-cert-${i}-number`}
                  label={tp("fields.number")}
                  errors={errors[`certificates.${i}.number`]}
                >
                  <Input
                    value={c.number ?? ""}
                    onChange={(e) =>
                      setC({ ...c, number: e.target.value || null })
                    }
                  />
                </Field>
                <Field
                  id={`pp-cert-${i}-valid`}
                  label={tp("fields.validUntil")}
                  errors={errors[`certificates.${i}.valid_until`]}
                >
                  <Input
                    type="date"
                    value={c.valid_until ?? ""}
                    onChange={(e) =>
                      setC({ ...c, valid_until: e.target.value || null })
                    }
                  />
                </Field>
                <Field
                  id={`pp-cert-${i}-file`}
                  label={tp("fields.file")}
                  errors={errors[`certificates.${i}.file_id`]}
                >
                  {fileSelect(
                    c.file_id,
                    (file_id) => setC({ ...c, file_id }),
                    `pp-cert-${i}-file`
                  )}
                </Field>
              </div>
            )}
            add={
              <Button
                type="button"
                variant="outline"
                onClick={() =>
                  set({
                    certificates: [...(draft.certificates ?? []), { name: "" }],
                  })
                }
              >
                {tp("addCertificate")}
              </Button>
            }
          />

          <ListEditor<Block>
            label={tp("fields.story")}
            items={draft.story ?? []}
            onChange={(story) => set({ story })}
            render={(b, setB, i) => (
              <StoryBlock
                block={b}
                onChange={setB}
                index={i}
                errors={errors}
                fileSelect={fileSelect}
              />
            )}
            add={(["text", "image", "facts", "quote"] as const).map((kind) => (
              <Button
                key={kind}
                type="button"
                variant="outline"
                onClick={() =>
                  set({
                    story: [
                      ...(draft.story ?? []),
                      kind === "text"
                        ? { kind, body: "" }
                        : kind === "image"
                          ? { kind, file_id: files[0]?.id ?? "" }
                          : kind === "facts"
                            ? { kind, items: [{ label: "", value: "" }] }
                            : { kind, text: "" },
                    ],
                  })
                }
              >
                {tp(`add.${kind}`)}
              </Button>
            ))}
          />

          <ApiError error={save.error} fallback={t("common.error")} />
          <div className="flex flex-wrap gap-2">
            <Button type="submit" disabled={save.isPending || !unsaved}>
              {t("actions.save")}
            </Button>
            <ConfirmDialog
              trigger={tp("publish")}
              destructive={false}
              title={tp("publishTitle")}
              description={tp("publishText", { code: lot.public_code })}
              confirm={tp("publish")}
              cancel={t("actions.cancel")}
              disabled={unsaved || publish.isPending}
              onConfirm={() => publish.mutate(vars)}
            />
            {resource.published ? (
              <ConfirmDialog
                trigger={tp("unpublish")}
                title={tp("unpublishTitle")}
                description={tp("unpublishText", { code: lot.public_code })}
                confirm={tp("unpublish")}
                cancel={t("actions.cancel")}
                disabled={unpublish.isPending}
                onConfirm={() => unpublish.mutate(vars)}
              />
            ) : null}
          </div>
          {unsaved ? (
            <p className="text-sm text-muted-foreground">{tp("saveFirst")}</p>
          ) : null}
          <ApiError
            error={publish.error ?? unpublish.error}
            fallback={t("common.error")}
          />
        </form>
      </section>

      <section
        aria-label={tp("preview")}
        className="flex min-w-0 flex-col gap-3"
      >
        <h2 className="heading-3 text-muted-foreground">{tp("preview")}</h2>
        <div className="rounded-lg border border-border p-4">
          <PassportView
            passport={toPublic(
              draft,
              files,
              resource.version,
              resource.published_at ?? new Date().toISOString()
            )}
            lot={{
              public_code: lot.public_code,
              status: lot.status,
              produced_on: lot.produced_on,
              best_before: lot.best_before,
              expires_on: lot.expires_on,
              origin: lot.origin,
              product: {
                name: product.data?.data.name ?? "",
                brand: product.data?.data.brand ?? null,
                short_description: product.data?.data.short_description ?? null,
              },
              farmers: [],
            }}
            locale={locale}
            t={tb}
          />
        </div>
      </section>
    </div>
  )
}

/** Percent typed by a person, stored as integer basis points; bad input is flagged, not guessed. */
function PercentInput({
  bp,
  onChange,
  id,
}: {
  bp: number | null | undefined
  onChange: (bp: number | null) => void
  id?: string
}) {
  const [text, setText] = React.useState(bpToPercent(bp))
  const bad = text !== "" && percentToBp(text) === null
  return (
    <Input
      id={id}
      inputMode="decimal"
      value={text}
      aria-invalid={bad || undefined}
      onChange={(e) => {
        setText(e.target.value)
        onChange(e.target.value === "" ? null : percentToBp(e.target.value))
      }}
    />
  )
}

function StoryBlock({
  block,
  onChange,
  index,
  errors,
  fileSelect,
}: {
  block: Block
  onChange: (b: Block) => void
  index: number
  errors: Record<string, string[]>
  fileSelect: (
    value: string | null | undefined,
    onChange: (id: string | null) => void,
    id: string
  ) => React.ReactElement<React.ComponentProps<"select">>
}) {
  const tp = useTranslations("passport")
  const id = (k: string) => `pp-story-${index}-${k}`
  const err = (k: string) => errors[`story.${index}.${k}`]
  return (
    <div className="flex flex-col gap-2">
      <p className="tag text-muted-foreground">{tp(`add.${block.kind}`)}</p>
      {block.kind === "text" ? (
        <>
          <Field
            id={id("title")}
            label={tp("fields.blockTitle")}
            errors={err("title")}
          >
            <Input
              value={block.title ?? ""}
              onChange={(e) =>
                onChange({ ...block, title: e.target.value || null })
              }
            />
          </Field>
          <Field id={id("body")} label={tp("fields.body")} errors={err("body")}>
            <Textarea
              value={block.body}
              onChange={(e) => onChange({ ...block, body: e.target.value })}
            />
          </Field>
        </>
      ) : block.kind === "image" ? (
        <>
          <Field
            id={id("file")}
            label={tp("fields.image")}
            errors={err("file_id")}
          >
            {fileSelect(
              block.file_id,
              (v) => onChange({ ...block, file_id: v ?? "" }),
              id("file")
            )}
          </Field>
          <Field
            id={id("caption")}
            label={tp("fields.caption")}
            errors={err("caption")}
          >
            <Input
              value={block.caption ?? ""}
              onChange={(e) =>
                onChange({ ...block, caption: e.target.value || null })
              }
            />
          </Field>
        </>
      ) : block.kind === "quote" ? (
        <>
          <Field
            id={id("text")}
            label={tp("fields.quote")}
            errors={err("text")}
          >
            <Textarea
              value={block.text}
              onChange={(e) => onChange({ ...block, text: e.target.value })}
            />
          </Field>
          <Field id={id("by")} label={tp("fields.by")} errors={err("by")}>
            <Input
              value={block.by ?? ""}
              onChange={(e) =>
                onChange({ ...block, by: e.target.value || null })
              }
            />
          </Field>
        </>
      ) : (
        <>
          {block.items.map((f, j) => (
            <div
              key={j}
              className="grid grid-cols-[1fr_1fr_auto] items-end gap-2"
            >
              <Field
                id={id(`label-${j}`)}
                label={tp("fields.factLabel")}
                errors={err(`items.${j}.label`)}
              >
                <Input
                  value={f.label}
                  onChange={(e) =>
                    onChange({
                      ...block,
                      items: block.items.map((x, k) =>
                        k === j ? { ...x, label: e.target.value } : x
                      ),
                    })
                  }
                />
              </Field>
              <Field
                id={id(`value-${j}`)}
                label={tp("fields.factValue")}
                errors={err(`items.${j}.value`)}
              >
                <Input
                  value={f.value}
                  onChange={(e) =>
                    onChange({
                      ...block,
                      items: block.items.map((x, k) =>
                        k === j ? { ...x, value: e.target.value } : x
                      ),
                    })
                  }
                />
              </Field>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() =>
                  onChange({
                    ...block,
                    items: block.items.filter((_, k) => k !== j),
                  })
                }
              >
                {tp("remove")}
              </Button>
            </div>
          ))}
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="self-start"
            onClick={() =>
              onChange({
                ...block,
                items: [...block.items, { label: "", value: "" }],
              })
            }
          >
            {tp("addFact")}
          </Button>
        </>
      )}
    </div>
  )
}
