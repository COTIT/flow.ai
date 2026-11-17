"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  importsCreate,
  importsUpload,
  importsValidate,
} from "@genesy/api-client/endpoints/imports"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { Label } from "@genesy/ui/components/label"
import { Notice } from "@genesy/ui/components/notice"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@genesy/ui/components/select"
import { PageHead } from "@/components/list"
import {
  autoMapping,
  chunkRows,
  FIELDS,
  fileHash,
  toRows,
  type Batch,
  type Mapping,
  type Profile,
} from "@/lib/imports/flow"

type Kind = "products_create" | "products_update" | "relations"
type Parsed = { columns: string[]; rows: { line: number; cells: string[] }[] }
const KEYS = ["product_nr_formatted", "barcode", "ean13", "reference"] as const
const NONE = "-"

/** One attempt's identity: a fresh client_ref and Idempotency-Key per call. */
const once = () => ({
  ref: crypto.randomUUID(),
  request: { headers: { "Idempotency-Key": crypto.randomUUID() } },
})

/** Pick a file, match its columns, then let the API check every row (nothing is saved yet). */
export default function NewImportPage() {
  const t = useTranslations()
  const router = useRouter()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const token = useSession((s) => s.token)
  const [kind, setKind] = React.useState<Kind>("products_create")
  const [matchKey, setMatchKey] =
    React.useState<(typeof KEYS)[number]>("barcode")
  const [file, setFile] = React.useState<{
    name: string
    hash: string
    sheet: Parsed
  }>()
  const [mapping, setMapping] = React.useState<Mapping>({})
  const [keyColumn, setKeyColumn] = React.useState<number>()
  const [reading, setReading] = React.useState(false)
  const [checking, setChecking] = React.useState(false)
  const [problem, setProblem] = React.useState<string>()
  const [error, setError] = React.useState<unknown>()
  const profile: Profile = kind === "relations" ? "relations" : "products"
  const fields = FIELDS[profile]
  const label = (f: string) => t(`imports.fields.${f}`)

  const kinds = (
    ["products_create", "products_update", "relations"] as const
  ).map((k) => ({
    value: k,
    label: t(`imports.profiles.${k}`),
  }))
  const keys = KEYS.map((k) => ({
    value: k,
    label: t(`imports.matchKeys.${k}`),
  }))
  const columns = [
    { value: NONE, label: t("imports.notMapped") },
    ...(file?.sheet.columns ?? []).map((c, i) => ({
      value: String(i),
      label: c || `#${i + 1}`,
    })),
  ]

  async function read(picked: File | undefined) {
    setFile(undefined)
    setProblem(undefined)
    setError(undefined)
    if (!picked) return
    setReading(true)
    try {
      const form = new FormData()
      form.append("file", picked, picked.name)
      const res = await fetch("/api/imports/parse", {
        method: "POST",
        body: form,
        headers: {
          Authorization: `Bearer ${token ?? ""}`,
          "X-Workspace-Id": workspaceId,
        },
      })
      const body = (await res.json().catch(() => ({}))) as {
        data?: Parsed
        message?: string
      }
      if (!res.ok || !body.data) {
        setProblem(body.message ?? t("common.error"))
        return
      }
      setFile({
        name: picked.name,
        hash: await fileHash(await picked.arrayBuffer()),
        sheet: body.data,
      })
      setMapping(autoMapping(body.data.columns, profile, label))
      setKeyColumn(undefined)
    } catch {
      setProblem(t("common.error"))
    } finally {
      setReading(false)
    }
  }

  async function check() {
    if (!file) return
    setProblem(undefined)
    setError(undefined)
    const update = kind === "products_update"
    if (!Object.values(mapping).some((v) => v !== undefined))
      return setProblem(t("imports.needMapping"))
    if (update && keyColumn === undefined)
      return setProblem(
        t("imports.needKey", { key: t(`imports.matchKeys.${matchKey}`) })
      )
    const rows = toRows(file.sheet, mapping, update ? keyColumn : undefined)
    const chunks = chunkRows(rows)
    setChecking(true)
    try {
      const c = once()
      const created = await importsCreate(
        workspaceId,
        {
          client_ref: c.ref,
          payload: {
            profile,
            filename: file.name.slice(0, 255),
            file_hash: file.hash,
            expected_rows: rows.length,
            mapping:
              profile === "relations"
                ? {}
                : {
                    operation: update ? "update" : "create",
                    match_key: matchKey,
                  },
          },
        },
        c.request
      )
      let batch = created.data as Batch
      for (const [index, chunk] of chunks.entries()) {
        const u = once()
        const res = await importsUpload(
          workspaceId,
          batch.id,
          index,
          {
            client_ref: u.ref,
            expected_version: batch.version,
            payload: { rows: chunk },
          },
          u.request
        )
        batch = res.data as Batch
      }
      const v = once()
      await importsValidate(
        workspaceId,
        batch.id,
        {
          client_ref: v.ref,
          expected_version: batch.version,
          payload: { expected_chunks: chunks.length },
        },
        v.request
      )
      router.push(`/settings/imports/${batch.id}`)
    } catch (e) {
      setError(e)
      setChecking(false)
    }
  }

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("imports.new")}
        description={t("imports.intro")}
        back={{ href: "/settings/imports", label: t("imports.back") }}
      />
      <Card className="flex flex-col gap-5 p-6 max-md:p-4">
        <div className="grid gap-4 md:grid-cols-2">
          <div className="flex flex-col gap-2">
            <Label>{t("imports.kind")}</Label>
            <Select
              items={kinds}
              value={kind}
              onValueChange={(v) => {
                if (!v) return
                setKind(v as Kind)
                if (file)
                  setMapping(
                    autoMapping(
                      file.sheet.columns,
                      v === "relations" ? "relations" : "products",
                      label
                    )
                  )
              }}
            >
              <SelectTrigger aria-label={t("imports.kind")} className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {kinds.map((o) => (
                  <SelectItem key={o.value} value={o.value}>
                    {o.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {profile === "products" ? (
            <div className="flex flex-col gap-2">
              <Label>{t("imports.matchKey")}</Label>
              <Select
                items={keys}
                value={matchKey}
                onValueChange={(v) =>
                  v && setMatchKey(v as (typeof KEYS)[number])
                }
              >
                <SelectTrigger
                  aria-label={t("imports.matchKey")}
                  className="w-full"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {keys.map((o) => (
                    <SelectItem key={o.value} value={o.value}>
                      {o.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          ) : null}
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="import-file">{t("imports.choose")}</Label>
          <input
            id="import-file"
            type="file"
            accept=".csv,.xlsx,text/csv,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            disabled={reading || checking}
            onChange={(e) => void read(e.target.files?.[0])}
            className="min-h-11 text-sm file:mr-3 file:rounded-md file:border file:border-border file:bg-transparent file:px-3 file:py-1.5 max-md:w-full"
          />
        </div>
        <div role="status">
          {reading ? (
            <Notice live={false} title={t("imports.reading")} />
          ) : null}
          {file ? (
            <Notice
              live={false}
              title={t("imports.preview", {
                count: file.sheet.rows.length,
                file: file.name,
              })}
            />
          ) : null}
        </div>
        {problem ? <Notice variant="error" title={problem} /> : null}
        <ApiError error={error} fallback={t("common.error")} />
        {file ? (
          <section aria-labelledby="map-title" className="flex flex-col gap-4">
            <div>
              <h2 id="map-title" className="font-semibold">
                {t("imports.mapTitle")}
              </h2>
              <p className="text-sm text-muted-foreground">
                {t("imports.mapHint")}
              </p>
            </div>
            <div className="grid gap-4 md:grid-cols-2">
              {kind === "products_update" ? (
                <ColumnPick
                  label={`${t("imports.keyColumn")}: ${t(`imports.matchKeys.${matchKey}`)}`}
                  items={columns}
                  value={keyColumn}
                  onChange={setKeyColumn}
                />
              ) : null}
              {fields.map((f) => (
                <ColumnPick
                  key={f}
                  label={t(`imports.fields.${f}`)}
                  items={columns}
                  value={mapping[f]}
                  onChange={(at) => setMapping((m) => ({ ...m, [f]: at }))}
                />
              ))}
            </div>
            <div>
              <Button disabled={checking} onClick={() => void check()}>
                {checking ? t("imports.checking") : t("imports.check")}
              </Button>
            </div>
          </section>
        ) : null}
      </Card>
    </>
  )
}

function ColumnPick({
  label,
  items,
  value,
  onChange,
}: {
  label: string
  items: { value: string; label: string }[]
  value: number | undefined
  onChange: (at: number | undefined) => void
}) {
  return (
    <div className="flex flex-col gap-2">
      <Label>{label}</Label>
      <Select
        items={items}
        value={value === undefined ? NONE : String(value)}
        onValueChange={(v) =>
          onChange(!v || v === NONE ? undefined : Number(v))
        }
      >
        <SelectTrigger aria-label={label} className="w-full">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {items.map((o) => (
            <SelectItem key={o.value} value={o.value}>
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
