"use client"

import Link from "next/link"
import { useEffect, useRef, useState } from "react"
import { useSearchParams } from "next/navigation"
import { useTranslations } from "next-intl"
import { useSession } from "@genesy/api-client/auth"
import { sessionStore } from "@genesy/api-client/session"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { Checkbox } from "@genesy/ui/components/checkbox"
import { Textarea } from "@genesy/ui/components/textarea"
import { PageHead } from "@/components/list"
import { Tile } from "@/components/bento"
import { useVoice } from "./provider"
import { requestFailureKey } from "./helpers"

class SecureRequestError extends Error {
  constructor(
    message: string,
    readonly status: number
  ) {
    super(message)
  }
}

type Schema = {
  type?: string | string[]
  title?: string
  description?: string
  format?: string
  properties?: Record<string, Schema>
  required?: string[]
  enum?: unknown[]
  items?: Schema
  maxLength?: number
  minimum?: number
  maximum?: number
  anyOf?: Schema[]
  oneOf?: Schema[]
  additionalProperties?: boolean | Schema
}
type Descriptor = {
  id: string
  summary: string
  parameters: { name: string; in: string; required: boolean; schema: Schema }[]
  body: Schema | null
}
function initial(schema: Schema): unknown {
  if (schema.properties)
    return Object.fromEntries(
      (schema.required ?? []).map((name) => [
        name,
        initial(schema.properties?.[name] ?? {}),
      ])
    )
  if (schema.type === "object" || schema.additionalProperties) return {}
  if (schema.type === "array") return []
  if (schema.type === "boolean") return false
  return ""
}
function ConfigurationInput({
  id,
  value,
  change,
}: {
  id: string
  value: unknown
  change: (value: unknown) => void
}) {
  const t = useTranslations("voice")
  const [text, setText] = useState(() =>
    value && typeof value === "object" ? JSON.stringify(value, null, 2) : "{}"
  )
  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs text-muted-foreground">{t("secureJsonHint")}</p>
      <Textarea
        id={id}
        rows={7}
        autoComplete="off"
        spellCheck={false}
        value={text}
        onChange={(e) => {
          setText(e.target.value)
          try {
            const parsed: unknown = JSON.parse(e.target.value)
            if (!parsed || typeof parsed !== "object") throw new Error()
            e.target.setCustomValidity("")
            change(parsed)
          } catch {
            e.target.setCustomValidity(t("secureJsonInvalid"))
          }
        }}
      />
    </div>
  )
}
function SchemaField({
  name,
  schema,
  value,
  change,
  required = false,
  level = 0,
}: {
  name: string
  schema: Schema
  value: unknown
  change: (value: unknown) => void
  required?: boolean
  level?: number
}) {
  const t = useTranslations("voice")
  const [included, setIncluded] = useState(required)
  const [cleared, setCleared] = useState(value === null)
  const nullable =
    (Array.isArray(schema.type) && schema.type.includes("null")) ||
    (schema.anyOf ?? schema.oneOf)?.some((variant) => variant.type === "null")
  const id = `secure-${name}`
  const alternatives = schema.anyOf ?? schema.oneOf
  const variants = alternatives?.filter((item) => item.type !== "null")
  const effective = variants?.length === 1 ? variants[0]! : schema
  const type = Array.isArray(effective.type)
    ? effective.type.find((item) => item !== "null")
    : effective.type
  const label =
    schema.title ??
    (name === "body"
      ? t("secureDetails")
      : name.split(".").at(-1)!.replaceAll("_", " "))
  const advanced =
    level > 7 ||
    Boolean(variants && variants.length > 1) ||
    (!type && !effective.properties && !effective.enum) ||
    (type === "object" && !Object.keys(effective.properties ?? {}).length)
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {!required ? (
        <label className="flex min-h-11 items-center gap-2 text-sm">
          <Checkbox
            className="size-7"
            checked={included}
            onCheckedChange={(checked) => {
              setIncluded(Boolean(checked))
              change(checked ? initial(effective) : undefined)
            }}
          />
          {t("secureOptional", { name: label })}
        </label>
      ) : null}
      {included ? (
        <>
          <label htmlFor={id} className="text-sm font-medium capitalize">
            {label}
            {required ? ` · ${t("secureRequired")}` : ""}
          </label>
          {schema.description ? (
            <p className="text-xs text-muted-foreground">
              {schema.description}
            </p>
          ) : null}
          {nullable ? (
            <label className="flex min-h-11 items-center gap-2 text-sm">
              <Checkbox
                className="size-7"
                checked={cleared}
                onCheckedChange={(checked) => {
                  setCleared(Boolean(checked))
                  change(checked ? null : initial(effective))
                }}
              />
              {t("secureClear", { name: label })}
            </label>
          ) : null}
          {cleared ? null : advanced ? (
            <ConfigurationInput id={id} value={value} change={change} />
          ) : effective.properties ? (
            <div className="flex flex-col gap-3 border-l border-(--hair) pl-3">
              {Object.entries(effective.properties).map(([key, child]) => (
                <SchemaField
                  key={key}
                  name={`${name}.${key}`}
                  schema={child}
                  required={effective.required?.includes(key)}
                  level={level + 1}
                  value={(value as Record<string, unknown> | undefined)?.[key]}
                  change={(next) => {
                    const result = {
                      ...((value as Record<string, unknown>) ?? {}),
                    }
                    if (next === undefined) delete result[key]
                    else result[key] = next
                    change(result)
                  }}
                />
              ))}
            </div>
          ) : type === "array" ? (
            <div className="flex flex-col gap-3">
              {(Array.isArray(value) ? value : []).map((item, i, rows) => (
                <div key={i} className="rounded-xl border border-(--hair) p-3">
                  <SchemaField
                    name={`${name}.${i}`}
                    schema={effective.items ?? { type: "string" }}
                    required
                    value={item}
                    level={level + 1}
                    change={(next) =>
                      change(rows.map((row, j) => (j === i ? next : row)))
                    }
                  />
                  <Button
                    type="button"
                    className="mt-2 min-h-11"
                    variant="outline"
                    onClick={() => change(rows.filter((_, j) => j !== i))}
                  >
                    {t("secureRemove")}
                  </Button>
                </div>
              ))}
              <Button
                type="button"
                className="min-h-11 self-start"
                variant="outline"
                onClick={() =>
                  change([
                    ...(Array.isArray(value) ? value : []),
                    initial(effective.items ?? { type: "string" }),
                  ])
                }
              >
                {t("secureAdd")}
              </Button>
            </div>
          ) : effective.enum ? (
            <select
              id={id}
              className="h-11 rounded-xl border border-(--field-border) bg-(--field) px-3"
              value={String(value ?? "")}
              required={required}
              onChange={(e) =>
                change(
                  effective.enum?.find(
                    (item) => String(item) === e.target.value
                  )
                )
              }
            >
              <option value="" />
              {effective.enum.map((option) => (
                <option key={String(option)} value={String(option)}>
                  {String(option)}
                </option>
              ))}
            </select>
          ) : type === "boolean" ? (
            <input
              id={id}
              type="checkbox"
              checked={Boolean(value)}
              className="size-11"
              onChange={(e) => change(e.target.checked)}
            />
          ) : type === "string" && (effective.maxLength ?? 0) > 500 ? (
            <Textarea
              id={id}
              required={required}
              maxLength={effective.maxLength}
              autoComplete="off"
              value={String(value ?? "")}
              onChange={(e) => change(e.target.value)}
            />
          ) : (
            <Input
              id={id}
              className="min-h-11"
              autoComplete="off"
              required={required}
              maxLength={effective.maxLength}
              min={effective.minimum}
              max={effective.maximum}
              step={type === "integer" ? 1 : "any"}
              type={
                type === "number" || type === "integer"
                  ? "number"
                  : /password|secret|token|pin|key/i.test(name)
                    ? "password"
                    : effective.format === "email"
                      ? "email"
                      : "text"
              }
              value={String(value ?? "")}
              onChange={(e) =>
                change(
                  type === "number" || type === "integer"
                    ? e.target.value === ""
                      ? ""
                      : Number(e.target.value)
                    : e.target.value
                )
              }
            />
          )}
        </>
      ) : null}
    </div>
  )
}
export function SecureVoiceForm() {
  const operationId = useSearchParams().get("operation") ?? ""
  const workspaceId = useSession((s) => s.workspaceId)
  return (
    <SecureForm
      key={`${workspaceId}:${operationId}`}
      operationId={operationId}
    />
  )
}
function SecureForm({ operationId }: { operationId: string }) {
  const t = useTranslations("voice")
  const workspaceId = useSession((s) => s.workspaceId)
  const token = useSession((s) => s.token)
  const voice = useVoice()
  const end = useRef(voice.end)
  const alive = useRef(true)
  const [descriptor, setDescriptor] = useState<Descriptor | null>(null)
  const [values, setValues] = useState<Record<string, unknown>>({})
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<unknown>(null)
  async function request(command: "describe" | "execute", args?: unknown) {
    const response = await fetch("/api/voice/secure", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({
        workspaceId,
        operationId,
        command,
        ...(args ? { arguments: args } : {}),
      }),
    })
    const body = await response.json()
    if (response.status === 401) sessionStore.getState().signOut("expired")
    if (!response.ok)
      throw new SecureRequestError(
        t(requestFailureKey(response.status)),
        response.status
      )
    return body
  }
  useEffect(() => {
    alive.current = true
    let mounted = true
    void end
      .current()
      .then(() => request("describe"))
      .then((body: Descriptor) => {
        if (!mounted) return
        setDescriptor(body)
        const path: Record<string, unknown> = {}
        const query: Record<string, unknown> = {}
        for (const parameter of body.parameters ?? [])
          if (parameter.required && parameter.name !== "workspace")
            (parameter.in === "path" ? path : query)[parameter.name] = initial(
              parameter.schema
            )
        setValues({
          path,
          query,
          ...(body.body ? { body: initial(body.body) } : {}),
        })
      })
      .catch((e: unknown) => {
        if (mounted)
          setError(e instanceof Error ? e.message : t("requestFailed"))
      })
    return () => {
      mounted = false
      alive.current = false
    }
    // Each operation/workspace gets a fresh, empty form after the microphone ends.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [operationId, workspaceId])
  return (
    <>
      <PageHead
        title={descriptor?.summary ?? t("secureTitle")}
        description={t("secureHint")}
      />
      <Tile id="secure-action" title={t("secureTitle")} span="">
        {error ? <Notice variant="error" title={error} /> : null}
        {!descriptor && !error ? (
          <p role="status">{t("secureLoading")}</p>
        ) : null}
        {descriptor && result === null ? (
          <form
            autoComplete="off"
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault()
              if (busy) return
              setBusy(true)
              setError("")
              void request("execute", values)
                .then((body) => {
                  if (!alive.current) return
                  setValues({})
                  setResult(body)
                })
                .catch((e: unknown) => {
                  if (!alive.current) return
                  setError(e instanceof Error ? e.message : t("secureUnknown"))
                  if (!(e instanceof SecureRequestError) || e.status >= 500) {
                    setValues({})
                    setResult({
                      status: "unknown",
                      message: t("secureUnknown"),
                    })
                  }
                })
                .finally(() => {
                  if (alive.current) setBusy(false)
                })
            }}
          >
            {descriptor.parameters
              .filter((parameter) => parameter.name !== "workspace")
              .map((parameter) => {
                const group = parameter.in === "path" ? "path" : "query"
                return (
                  <SchemaField
                    key={`${group}.${parameter.name}`}
                    name={`${group}.${parameter.name}`}
                    schema={parameter.schema}
                    required={parameter.required}
                    value={
                      (values[group] as Record<string, unknown>)?.[
                        parameter.name
                      ]
                    }
                    change={(value) =>
                      setValues((current) => {
                        const section = {
                          ...(current[group] as Record<string, unknown>),
                        }
                        if (value === undefined) delete section[parameter.name]
                        else section[parameter.name] = value
                        return { ...current, [group]: section }
                      })
                    }
                  />
                )
              })}
            {descriptor.body ? (
              <SchemaField
                name="body"
                schema={descriptor.body}
                required
                value={values.body}
                change={(body) =>
                  setValues((current) => ({ ...current, body }))
                }
              />
            ) : null}
            <Button
              className="min-h-11 self-start"
              type="submit"
              disabled={busy}
            >
              {t(busy ? "working" : "secureSubmit")}
            </Button>
          </form>
        ) : null}
        {result !== null ? (
          <>
            <Notice
              variant={
                typeof result === "object" &&
                result &&
                "status" in result &&
                result.status !== "completed"
                  ? "warning"
                  : "success"
              }
              title={
                typeof result === "object" &&
                result &&
                "status" in result &&
                result.status !== "completed"
                  ? t("secureUnknown")
                  : t("secureDone")
              }
            />
            <pre className="max-h-96 overflow-auto rounded-xl bg-(--glass-subtle) p-4 text-sm break-all whitespace-pre-wrap">
              {JSON.stringify(result, null, 2)}
            </pre>
          </>
        ) : null}
        <Link
          href="/voice"
          className="inline-flex min-h-11 items-center text-sm font-medium underline"
        >
          {t("secureBack")}
        </Link>
      </Tile>
    </>
  )
}
