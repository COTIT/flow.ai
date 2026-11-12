import { createHash } from "node:crypto"
import { describe, expect, it } from "vitest"
import { locales } from "@genesy/i18n/locales"
import { loadOfficeMessages } from "./messages"

// Canonical JSON (sorted keys) so the pin checks content, not key order.
const canon = (v: unknown): unknown =>
  Array.isArray(v)
    ? v.map(canon)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.keys(v)
            .sort()
            .map((k) => [k, canon((v as Record<string, unknown>)[k])])
        )
      : v
const hash = (v: unknown) =>
  createHash("sha256")
    .update(JSON.stringify(canon(v)))
    .digest("hex")

// Office's `voice` texts as they were in apps/office/messages at a432dec17, with one deliberate,
// approved change since (P3-3): operationActions.run "Run action for {record}" → "Review this
// change" (and nl/sw). Before it: en f67b52c1…, nl ae3d1bcc…, sw 76e478f6… (still match with the
// old `run` text restored), so any other change to an existing text still fails here.
// Second approved change (P6 plain words, for older non-technical users): textInstead,
// enableVoice, review, approve, cancel, reviewHint, spokenHint, fileStates.{selected,uploaded,
// submitted,failed}, activity.{failed,executing,uploading,checking,unknown,queued},
// taskStates.unknown (en/nl/sw). Before it: en 7d992577…, nl d54d2135…, sw ef5c2c65… (still
// match with those texts restored), so any other change to an existing text still fails here.
const VOICE = {
  en: "6b3e4de785ff3becdf1512358a7f1f9c0aa73271d4cc7518aa7653edf7e38da4",
  nl: "464cfdf16b6e1e6503f6f836c2f171a3e7559f914e1a0637462d0d3c9ad026e7",
  sw: "4d548f07d8edcd6cc6b53d5a29d2153dff2239c60ad648e963ce6d15ba3d3547",
}

// Review-card field names added for storefront design facts (body, category_id existed).
const FLOW_NOTICED_SECTIONS = [
  "stock_below_minimum",
  "orders_open",
  "invoices_overdue",
  "till_open_shifts",
  "lots_quarantined",
  "webhooks_failed",
]
// Added since (P3-3): review-row labels and record states of "what changes".
const P33_FIELDS = [
  "stock",
  "stock_from",
  "stock_to",
  "minimal",
  "maximum",
  "picked",
  "returned",
  "refund",
  "payment",
  "first_name",
  "last_name",
  "company_name",
]
const P33_VALUES = ["open", "returned", "partially_returned"]
const DESIGN_FIELDS = [
  "theme",
  "heading",
  "layout",
  "category",
  "limit",
  "columns",
  "section",
  "position",
  "text",
]

describe("Office messages", () => {
  it.each(locales)(
    "%s keeps the voice texts it had before the move",
    async (locale) => {
      const messages = await loadOfficeMessages(locale)
      const voice = messages.voice as {
        pages: Record<string, unknown>
        resources: Record<string, unknown>
        reviewFields: Record<string, unknown>
        values: Record<string, unknown>
        operationActions: Record<string, unknown>
        results: {
          sections: Record<string, unknown>
          counts: Record<string, unknown>
        }
      }
      // Added since (2C): the read-only storefront page of the assistant's page tool.
      const { storefront, ...pages } = voice.pages
      expect(storefront).toBeDefined()
      // Added since (Phase 3): storefront design review labels.
      const { storefront_design, ...resources } = voice.resources
      expect(storefront_design).toBeDefined()
      const reviewFields = { ...voice.reviewFields }
      for (const key of DESIGN_FIELDS) {
        expect(reviewFields[key], key).toBeDefined()
        delete reviewFields[key]
      }
      for (const key of P33_FIELDS) {
        expect(reviewFields[key], key).toBeDefined()
        delete reviewFields[key]
      }
      const values = { ...voice.values }
      for (const key of P33_VALUES) {
        expect(values[key], key).toBeDefined()
        delete values[key]
      }
      // Added since (Flow v9 V1): Flow noticed section and mark labels of the attention view.
      const results = voice.results
      const sections = { ...results.sections }
      for (const key of FLOW_NOTICED_SECTIONS) {
        expect(sections[key], key).toBeDefined()
        delete sections[key]
      }
      const { acknowledged, snoozed, ...counts } = results.counts
      expect(acknowledged).toBeDefined()
      expect(snoozed).toBeDefined()
      // Added since (voice results): labels of the breakdown and comparison pictures.
      // P2: labels of the stock, search and record-detail pictures.
      const { breakdown, compare, stock, search, document, facts, totals, till, ...unchanged } = results as typeof results &
        Record<"breakdown" | "compare" | "stock" | "search" | "document" | "facts" | "totals" | "till", unknown>
      for (const added of [breakdown, compare, stock, search, document, facts, totals, till]) expect(added).toBeDefined()
      // Added since (P6-1/SW6): plain-words lines for a dropped line, offline, no microphone.
      // P6-5: an approval made offline was never sent.
      const {
        connectionLost,
        offline,
        offlineNotSent,
        microphoneMissing,
        // Added since (P3-3): plain titles of the reviewed operations.
        operationTitles,
        // Added since (P6 plain words): the short End label on phones.
        endShort,
        ...rest
      } = voice as typeof voice & Record<string, unknown>
      expect(operationTitles).toBeDefined()
      expect(endShort).toBeDefined()
      for (const text of [
        connectionLost,
        offline,
        offlineNotSent,
        microphoneMissing,
      ])
        expect(text).toBeDefined()
      expect(
        hash({
          ...rest,
          pages,
          resources,
          reviewFields,
          values,
          results: { ...unchanged, sections, counts },
        })
      ).toBe(VOICE[locale])
    }
  )

  it.each(locales)(
    "%s gets voice from @genesy/i18n, not its own file",
    async (locale) => {
      const own = (await import(`../messages/${locale}.json`)).default
      expect(own).not.toHaveProperty("voice")
    }
  )
})
