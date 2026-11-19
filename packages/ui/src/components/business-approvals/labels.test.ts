import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"

import { CODES, blockedReason, errorText, label } from "./labels"

const messages = (locale: string) =>
  JSON.parse(
    readFileSync(
      join(__dirname, `../../../../i18n/messages/${locale}.json`),
      "utf8"
    )
  ).businessApprovals as Record<string, Record<string, string>>

const en = messages("en")
const t = (key: string, values?: Record<string, string | number>) => {
  const [group, name] = key.split(".") as [string, string]
  let text = en[group]?.[name]
  if (text === undefined) throw new Error(`missing message ${key}`)
  for (const [k, v] of Object.entries(values ?? {}))
    text = text.replace(`{${k}}`, String(v))
  return text
}

describe("business approval labels", () => {
  it("has English text for every code and an unknown fallback per group", () => {
    for (const [group, codes] of Object.entries(CODES))
      for (const code of [...codes, "unknown"])
        expect(en[group]?.[code], `${group}.${code}`).toBeTruthy()
  })

  it("never shows a raw code, even an unknown one", () => {
    expect(
      label(t, "executionStatus", "awaiting_reconciliation")
    ).not.toContain("_")
    expect(label(t, "executionStatus", "brand_new_state")).toBe(
      en.executionStatus!.unknown
    )
    expect(
      label(t, "connectionReason", "probe_failed:purchase_request.read")
    ).toBe(en.connectionReason!.probe_failed)
  })

  it("names the unmapped entity and its source id", () => {
    expect(
      blockedReason(t, { code: "unmapped", entity: "vendor", source_id: "43" })
    ).toContain("43")
  })

  it("maps API errors by code, then by status", () => {
    const err = (status: number, data: object) => ({
      response: { status, data },
    })
    expect(
      errorText(t, err(409, { code: "approval_stale", message: "x" }))
    ).toBe(en.error!.approval_stale)
    expect(
      errorText(t, err(403, { message: "This action is unauthorized." }))
    ).toBe(en.error!.forbidden)
    expect(
      errorText(t, err(422, { message: "The comment field is required." }))
    ).toBe("The comment field is required.")
    expect(errorText(t, new Error("network"))).toBe(en.error!.unknown)
  })
})
