import { createHash } from "node:crypto"
import { describe, expect, it } from "vitest"
import { locales } from "@verzo/i18n/locales"
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
  en: "ea53294f3b18cbefa956fc60c7429aad234ee9b2c3b383400d0c2f1f1a19f17c",
  nl: "b37ed53adaca8f47c117a00398009d5555d8657fa33cd6396f58dea4681d2c6f",
  sw: "928d29c2d3324381d1f1cb85fd113b6eb6dd8f8ba8984b647b4e013026ba988c",
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

const ERP_PAGES = [
  "accounting",
  "accounting_reports",
  "purchases",
  "supplier_bills",
  "supplier_payments",
  "purchase_returns",
  "sales_returns",
  "stock_counts",
]

describe("Office messages", () => {
  it.each(locales)(
    "%s keeps the reviewed voice texts",
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
      const { storefront, ...pagesAfter2C } = voice.pages
      expect(storefront).toBeDefined()
      // Added since (M7-E): the ERP pages the assistant's page tool can open.
      const pages = { ...pagesAfter2C }
      for (const key of ERP_PAGES) {
        expect(pages[key], key).toBeDefined()
        delete pages[key]
      }
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
        // Added for opt-in history: distinct call control and pending delivery labels.
        endCall,
        delivery,
        // Added since chart reports: download and selection labels.
        reports,
        ...rest
      } = voice as typeof voice & Record<string, unknown>
      expect(operationTitles).toBeDefined()
      expect(endShort).toBeDefined()
      expect(endCall).toEqual(expect.any(String))
      expect(delivery).toEqual({
        queued: expect.any(String),
        sent_unconfirmed: expect.any(String),
      })
      expect(reports).toBeDefined()
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
    "%s gets voice from @verzo/i18n, not its own file",
    async (locale) => {
      const own = (await import(`../messages/${locale}.json`)).default
      expect(own).not.toHaveProperty("voice")
    }
  )
})
