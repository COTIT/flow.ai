import type { Frame, Page } from "@playwright/test"

import document from "../../../../apps/api/tests/fixtures/storefront-design-v1.json" with { type: "json" }
import { expect, test as base } from "../../src/test"
import { FakeApi, paged } from "../office/fake-api"
import { AMINA, ILUNDO, RUAHA } from "./api"

export const draft = "01950000-0000-7000-8000-000000000002"
export const published = "01950000-0000-7000-8000-000000000001"
export const restored = "01950000-0000-7000-8000-000000000003"
export const otherDraft = "01950000-0000-7000-8000-000000000009"
export const root = (ws = "w1") => `/workspace/${ws}/storefront/pages/home`
// Screen readers hear the revision number and save date, never the UUID.
export const revisionButton = (n: number) =>
  new RegExp(`^Preview revision ${n}, saved `)

/** Owner of Ilundo, admin of Ruaha: may review both. */
export const owner = {
  ...AMINA,
  permissions: [
    { workspace: "w1", permissions: ["workspace_owner"] },
    { workspace: "w2", permissions: ["workspace_admin"] },
  ],
}
/** A plain member: the API answers 403 to every design call. */
export const member = {
  ...AMINA,
  permissions: [{ workspace: "w1", permissions: ["product.list"] }],
}

const product = (n: number, stock_status: string) => ({
  id: `01950000-0000-7000-8000-00000000010${n}`,
  name: `Honey jar ${n}`,
  short_description: "Raw forest honey",
  price_gross: 1250,
  currency: "EUR",
  currency_decimals: 2,
  stock_status,
  variants: [],
  images: [],
})
/** Live catalog rows the preview answers with (no images: nothing leaves the test). */
export const products = [
  product(1, "in_stock"),
  product(2, "low"),
  product(3, "sold_out"),
  product(4, "in_stock"),
]

/**
 * Starts the top page logged in. Unlike seedSession, it skips frames: the preview frame shares
 * this origin's storage and must read what the page left there (e.g. after a workspace switch).
 */
export async function seedTopSession(page: Page) {
  const value = JSON.stringify({
    state: {
      token: "e2e-token",
      workspaceIds: ["w1", "w2"],
      workspaceId: "w1",
    },
    version: 0,
  })
  await page.addInitScript((v) => {
    if (window.top === window) localStorage.setItem("genesy.session", v)
  }, value)
}

/** Drops the saved login before any app script runs (registered after the seed, so it wins). */
export async function loggedOut(page: Page) {
  await page.addInitScript(() => localStorage.removeItem("genesy.session"))
}

/**
 * Ilundo's design: draft revision 2, published revision 1. Publish moves the published pointer,
 * restore makes revision 3 the draft; both bump the version, as the API does.
 */
export function designs(api: FakeApi, ws = "w1", heading?: string) {
  let version = 2
  let currentDraft = ws === "w1" ? draft : otherDraft
  let currentPublished: string | null = ws === "w1" ? published : null
  const doc = heading
    ? {
        ...document,
        sections: document.sections.map((s) =>
          s.type === "hero" ? { ...s, props: { ...s.props, heading } } : s
        ),
      }
    : document
  const state = () => ({
    data: {
      version,
      draft_revision_id: currentDraft,
      published_revision_id: currentPublished,
      draft: { revision_id: currentDraft, document: doc },
    },
  })
  const ids = ws === "w1" ? [draft, published] : [otherDraft]
  api
    .on("GET", root(ws), state)
    .on("GET", `${root(ws)}/revisions`, () =>
      paged(
        ids.map((revision_id) => ({
          revision_id,
          created_at: "2026-11-09T08:00:00Z",
          created_by: "u1",
          restored_from_revision_id: null,
          was_published: revision_id === published,
        }))
      )
    )
    .on("GET", `${root(ws)}/preview`, ({ query }) => ({
      data: {
        version,
        revision_id: query.get("revision_id"),
        document: doc,
        collections: [{ section_id: "featured-products", products }],
      },
    }))
    .on("POST", `${root(ws)}/publish`, ({ body }) => {
      currentPublished = (body as { revision_id: string }).revision_id
      version++
      return state()
    })
    .on("POST", `${root(ws)}/restore`, () => {
      currentDraft = restored
      version++
      return { status: 201, body: state() }
    })
}

/**
 * The edit dry run Flow calls before showing a design review: "ok" answers the base document with
 * the requested heading and theme applied (nothing stored), "conflict" a 409 as when the draft
 * moved, "broken" a 500. Returns the bodies it was sent.
 */
export function designEdits(api: FakeApi, mode: "ok" | "conflict" | "broken" = "ok", ws = "w1") {
  const sent: unknown[] = []
  api.on("POST", `${root(ws)}/edits/preview`, ({ body }) => {
    sent.push(body)
    if (mode === "conflict")
      return { status: 409, body: { message: "The design changed. Reload before saving or publishing." } }
    if (mode === "broken") return { status: 500, body: { message: "Internal error" } }
    const edit = body as { expected_version: number; changes: { op: string; heading?: string; theme?: string }[] }
    const heading = edit.changes.find((c) => c.heading)?.heading ?? "Fresh this week"
    const theme = edit.changes.find((c) => c.theme)?.theme ?? document.theme
    return {
      data: {
        version: edit.expected_version,
        base_revision_id: draft,
        document: {
          ...document,
          theme,
          sections: document.sections.map((s) =>
            s.type === "product_grid" ? { ...s, props: { ...s.props, heading } } : s
          ),
        },
        collections: [{ section_id: "featured-products", products }],
        changes: [],
      },
    }
  })
  return sent
}

/** The proposal frame once it has loaded (fed by message, not by URL). */
export async function proposalFrame(page: Page): Promise<Frame> {
  await expect(page.locator('iframe[title="Proposed shop preview"]')).toBeVisible()
  await expect
    .poll(() => page.frames().some((f) => f.url().includes("/storefront/preview?proposal=1")))
    .toBe(true)
  return page.frames().find((f) => f.url().includes("/storefront/preview?proposal=1"))!
}

/** The preview frame once it has loaded (the iframe's own document). */
export async function previewFrame(page: Page): Promise<Frame> {
  await expect(page.locator('iframe[title="Shop preview"]')).toBeVisible()
  await expect
    .poll(() =>
      page.frames().some((f) => f.url().includes("/storefront/preview"))
    )
    .toBe(true)
  return page.frames().find((f) => f.url().includes("/storefront/preview"))!
}

/** Flow's guarded test with a signed-in owner of two workspaces and a recording fake API. */
export const test = base.extend<{ api: FakeApi }>({
  api: async ({ page }, use) => {
    await seedTopSession(page)
    const api = new FakeApi()
      .on("GET", "/me", () => ({ data: { user: owner } }))
      .on("GET", "/me/workspaces", () => ({ data: [ILUNDO, RUAHA] }))
      .on("POST", "/logout", () => ({ status: 204 }))
    await page.route("**/v1/**", (route) => api.handle(route))
    await use(api)
  },
})

export { expect }
