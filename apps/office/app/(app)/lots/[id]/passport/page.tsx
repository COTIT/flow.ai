"use client"

import * as React from "react"
import { ExternalLinkIcon, PlusIcon } from "lucide-react"
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
import { Badge } from "@genesy/ui/components/badge"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { PassportView } from "@genesy/ui/components/passport-view"
import { Tabs, TabsList, TabsTrigger } from "@genesy/ui/components/tabs"
import { Textarea } from "@genesy/ui/components/textarea"
import { cn } from "@genesy/ui/lib/utils"
import { useCan } from "@/lib/can"
import { Bento, DetailSkeleton, Tile, type Tone } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { PageHead } from "@/components/list"
import { Capped, Done, LoadError, useDone } from "@/components/states"

import { ListEditor } from "./list-editor"
import { bpToPercent, percentToBp, toPublic } from "./to-public"

const blockTone: Record<Block["kind"], Tone> = {
  text: "slate",
  image: "slate",
  facts: "gold",
  quote: "teal",
}

/** One line per story block for its row, like the mockup's "The harvest starts in June…". */
function blockSummary(b: Block) {
  switch (b.kind) {
    case "text":
      return b.title || b.body
    case "image":
      return b.caption ?? ""
    case "quote":
      return b.text
    default:
      return b.items
        .map((f) => [f.label, f.value].filter(Boolean).join(" "))
        .filter(Boolean)
        .join(" · ")
  }
}

const gold =
  "bg-[image:var(--gold-gradient)] font-semibold text-gold-foreground hover:opacity-90"

export default function PassportPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const lot = useLotsGet(workspaceId, id, enabled)
  const passport = useLotsPassportGet(workspaceId, id, enabled)
  // shortcut: pickers and preview see the first 100 files; add file search when workspaces hold more.
  const listFiles = useCan("file.list")
  const files = useFilesList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: Boolean(workspaceId) && listFiles } }
  )
  // Lives here, not in Editor: Editor remounts after each save, which would wipe the notice.
  const done = useDone()
  // The editor starts from a settled read, never a cached copy still being read again, and
  // remounts only after its own writes: a background read must not wipe what is being typed.
  const [settled, setSettled] = React.useState(false)
  const [version, setVersion] = React.useState(0)
  if (!settled && passport.isSuccess && !passport.isFetching) setSettled(true)

  if (lot.isError || passport.isError)
    return (
      <LoadError
        backHref="/lots"
        error={lot.error ?? passport.error}
        onRetry={() => {
          void lot.refetch()
          void passport.refetch()
        }}
      />
    )
  if (lot.isPending || passport.isPending || !settled) return <DetailSkeleton />
  return (
    <Editor
      done={done}
      key={version}
      // A fresh copy of the server's draft after every save, publish or unpublish.
      onWritten={() => setVersion((v) => v + 1)}
      lot={lot.data.data}
      resource={passport.data.data}
      files={files.data?.data ?? []}
      filesMeta={files.data?.meta}
    />
  )
}

function Editor({
  lot,
  resource,
  files,
  filesMeta,
  done,
  onWritten,
}: {
  lot: LotResource
  resource: PassportResource
  files: FileResource[]
  filesMeta?: { total: number; per_page: number }
  done: ReturnType<typeof useDone>
  onWritten: () => void
}) {
  const t = useTranslations()
  const tp = useTranslations("passport")
  const tb = useTranslations("batch")
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  // Name and brand for the preview, when the member may read products.
  const product = useProductsGet(workspaceId, lot.product_id, undefined, {
    query: { enabled: useCan("product.view") },
  })
  const [draft, setDraft] = React.useState<Passport>(resource.draft)
  const set = (patch: Partial<Passport>) =>
    setDraft((d) => ({ ...d, ...patch }))
  // Typed text per coordinate; the map point exists only once both are filled in.
  const [coords, setCoords] = React.useState({
    lat: draft.map ? String(draft.map.lat) : "",
    lng: draft.map ? String(draft.map.lng) : "",
  })
  // Phones show one side at a time; md and up show both and hide the tabs.
  const [view, setView] = React.useState<"edit" | "preview">("edit")
  const [confirm, setConfirm] = React.useState<"publish" | "unpublish" | null>(
    null
  )
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getLotsPassportGetQueryKey(workspaceId, lot.id),
    })
  const vars = { workspace: workspaceId, lot: lot.id }
  const ok = (message: string) => ({
    onMutate: done.clear,
    onSuccess: async () => {
      done.show(message)
      await refresh()
      onWritten()
    },
  })
  const save = useLotsPassportSave({ mutation: ok(tp("done.saved")) })
  const publish = useLotsPassportPublish({ mutation: ok(tp("done.published")) })
  const unpublish = useLotsPassportUnpublish({
    mutation: ok(tp("done.unpublished")),
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

  const originField = (k: "country" | "region" | "place") => (
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
  )

  const coordField = (k: "lat" | "lng") => (
    <Field
      key={k}
      id={`pp-map-${k}`}
      label={tp(`fields.${k}`)}
      errors={errors[`map.${k}`]}
    >
      <Input
        type="number"
        step="any"
        className="tabular-nums"
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
  )

  const addButton = (label: string, onClick: () => void) => (
    <Button
      key={label}
      type="button"
      variant="outline"
      size="sm"
      onClick={onClick}
    >
      <PlusIcon />
      {label}
    </Button>
  )

  return (
    <>
      <PageHead
        back={{ href: `/lots/${lot.id}`, label: tp("back", { code: lot.code }) }}
        title={t("lots.passport")}
        description={unsaved ? tp("saveFirst") : undefined}
        actions={
          <>
            {resource.published ? (
              <Badge
                title={
                  resource.published_at
                    ? tp("publishedState", {
                        version: resource.version,
                        date: new Date(resource.published_at).toLocaleString(
                          locale
                        ),
                      })
                    : undefined
                }
              >
                {tp("publishedBadge", { version: resource.version })}
              </Badge>
            ) : (
              <Badge variant="slate" data-dot="false">
                {tp("draftBadge")}
              </Badge>
            )}
            {resource.published ? (
              <Button
                variant="outline"
                disabled={unpublish.isPending}
                onClick={() => setConfirm("unpublish")}
              >
                {tp("unpublish")}
              </Button>
            ) : null}
            <Button
              type="submit"
              form="passport-form"
              variant="outline"
              disabled={save.isPending || !unsaved}
            >
              {tp("saveDraft")}
            </Button>
            <Button
              className={gold}
              disabled={unsaved || publish.isPending}
              onClick={() => setConfirm("publish")}
            >
              {tp("publishVersion", { version: resource.version + 1 })}
            </Button>
          </>
        }
      />
      <ConfirmDialog
        open={confirm === "publish"}
        onOpenChange={(open) => !open && setConfirm(null)}
        destructive={false}
        title={tp("publishTitle")}
        description={tp("publishText", { code: lot.public_code })}
        confirm={tp("publish")}
        cancel={t("actions.cancel")}
        onConfirm={() => publish.mutate(vars)}
      />
      <ConfirmDialog
        open={confirm === "unpublish"}
        onOpenChange={(open) => !open && setConfirm(null)}
        title={tp("unpublishTitle")}
        description={tp("unpublishText", { code: lot.public_code })}
        confirm={tp("unpublish")}
        cancel={t("actions.cancel")}
        onConfirm={() => unpublish.mutate(vars)}
      />
      <Done message={done.message} />
      {resource.published && unpublished ? (
        <Notice variant="info" title={tp("draftDiffersTitle")}>
          {tp("draftDiffersBody", { version: resource.version })}
        </Notice>
      ) : null}
      <ApiError
        error={save.error ?? publish.error ?? unpublish.error}
        fallback={t("common.error")}
      />

      <Tabs
        className="md:hidden"
        value={view}
        onValueChange={(v) => setView(v as "edit" | "preview")}
      >
        <TabsList
          aria-label={tp("view")}
          className="gap-1 rounded-[14px] border border-(--hair) bg-(--glass-subtle) p-1"
        >
          {(["edit", "preview"] as const).map((v) => (
            <TabsTrigger
              key={v}
              value={v}
              className="h-11 rounded-[10px] px-3.5 text-[13px] data-active:border-(--glass-border) data-active:bg-(--glass-strong) dark:data-active:border-(--glass-border) dark:data-active:bg-(--glass-strong)"
            >
              {tp(`tabs.${v}`)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <Bento>
        <form
          id="passport-form"
          className={cn(
            "col-span-7 flex min-w-0 flex-col gap-4",
            view === "preview" && "max-md:hidden"
          )}
          onSubmit={(e) => {
            e.preventDefault()
            save.mutate({ ...vars, data: draft })
          }}
        >
          <Tile id="pp-story-card" title={tp("fields.story")} span="">
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
            <Capped meta={filesMeta} />
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
          </Tile>

          <Tile id="pp-origin-card" title={tp("originMap")} span="">
            <div className="grid gap-4 sm:grid-cols-2">
              {originField("place")}
              {originField("country")}
              {coordField("lat")}
              {coordField("lng")}
              {originField("region")}
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
            </div>
          </Tile>

          <ListEditor<Block>
            id="pp-blocks-card"
            title={tp("storyBlocks")}
            label={tp("fields.story")}
            items={draft.story ?? []}
            onChange={(story) => set({ story })}
            kind={(b) => ({ label: tp(`add.${b.kind}`), tone: blockTone[b.kind] })}
            summary={blockSummary}
            render={(b, setB, i) => (
              <StoryBlock
                block={b}
                onChange={setB}
                index={i}
                errors={errors}
                fileSelect={fileSelect}
              />
            )}
            add={(["text", "image", "facts", "quote"] as const).map((kind) =>
              addButton(tp(`add.${kind}`), () =>
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
              )
            )}
          />

          <ListEditor
            id="pp-certs-card"
            title={tp("fields.certificates")}
            label={tp("fields.certificates")}
            items={draft.certificates ?? []}
            onChange={(certificates) => set({ certificates })}
            kind={(_, i) => ({ label: String(i + 1), tone: "slate" })}
            summary={(c) => c.name}
            render={(c, setC, i) => (
              <div className="grid gap-4 sm:grid-cols-2">
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
            add={addButton(tp("addCertificate"), () =>
              set({
                certificates: [...(draft.certificates ?? []), { name: "" }],
              })
            )}
          />

          <ListEditor
            id="pp-materials-card"
            title={tp("fields.materials")}
            label={tp("fields.materials")}
            items={draft.materials ?? []}
            onChange={(materials) => set({ materials })}
            kind={(_, i) => ({ label: String(i + 1), tone: "slate" })}
            summary={(m) => m.name}
            render={(m, setM, i) => (
              <div className="grid gap-4 sm:grid-cols-3">
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
            add={addButton(tp("addMaterial"), () =>
              set({ materials: [...(draft.materials ?? []), { name: "" }] })
            )}
          />
        </form>

        <section
          aria-labelledby="pp-preview"
          className={cn(
            "col-span-5 flex min-w-0 flex-col gap-3",
            view === "edit" && "max-md:hidden"
          )}
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2
              id="pp-preview"
              className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
            >
              {tp("livePreview")}
            </h2>
            {resource.published && shopUrl ? (
              <a
                className={buttonVariants({ variant: "ghost", size: "sm" })}
                href={`${shopUrl}/b/${lot.public_code}`}
                target="_blank"
                rel="noreferrer"
              >
                {tp("open", { code: lot.public_code })}
                <ExternalLinkIcon aria-hidden="true" />
              </a>
            ) : null}
          </div>
          <Card className="gap-0 overflow-hidden p-5">
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
          </Card>
        </section>
      </Bento>
    </>
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
