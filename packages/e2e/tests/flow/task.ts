import type { Page, Route } from "@playwright/test"

import taskEn from "../../../../apps/flow/messages/en/task.json" with { type: "json" }
import type { MockReply } from "../../src/api-mock"
import { expect } from "../../src/test"
import { s as shell } from "./shell"
import { chat, startText, type Agent } from "./voice"

/**
 * SIMULATED Office answers for Flow's task specs: Office-shaped reviews and outcomes (top-level
 * actionId, Rust review facts under details.current), routed in the browser. Read-backs go to
 * the mocked /v1 API. No live agent, no real backend.
 */

/** Flow task texts (capsule, choice, receipt, batch, resume). */
export const k = taskEn.task

/** The active-task line (dock on home, scene bar in scenes). */
export const capsule = (page: Page) => page.getByRole("status", { name: k.label })
/** The receipt an approved action turns into. */
export const receipt = (page: Page) => page.locator("[data-receipt]")
/** Where the task shows (P5): the card in the conversation, or the Results screen ("Show all"). */
export const scene = (page: Page) =>
  page
    .locator("[data-inline-task]:not(:empty)")
    .or(page.getByRole("region", { name: chat.results }))
    .first()

/**
 * The task line as the person sees it once the conversation is hidden (P5: while the card is on
 * screen the capsule is hidden, so the line's state is read where it shows).
 */
export async function taskLine(page: Page) {
  // Beside a scene: hide the rail or sheet; on home: leave the full conversation.
  const hide = page.getByRole("button", { name: shell.hideConversation }).filter({ visible: true }).first()
  const home = page.getByRole("button", { name: shell.backHome }).filter({ visible: true }).first()
  // The way out can move as the first reply lands (the orb steps aside): find it again each try.
  await expect(async () => {
    if (!(await capsule(page).isVisible()))
      if (new URL(page.url()).pathname === "/") await home.click({ timeout: 1_000 })
      else await hide.click({ timeout: 1_000 })
    await expect(capsule(page)).toBeVisible({ timeout: 1_000 })
  }).toPass()
  return capsule(page)
}
/** Back from the hidden conversation to the card, by the capsule's own button. */
export async function backToCard(page: Page) {
  await capsule(page).getByRole("button").first().click()
  await expect(scene(page)).toBeVisible()
}

export const HONEY = [
  { id: "p-250", label: "Raw Honey 250g", detail: "HNY-250" },
  { id: "p-500", label: "Raw Honey 500g", detail: "HNY-500" },
  { id: "p-1000", label: "Raw Honey 1kg", detail: "HNY-1000" },
]
const products = {
  status: "completed",
  data: {},
  view: {
    kind: "records",
    resource: "products",
    total: 3,
    href: "/products",
    rows: HONEY.map((p) => ({
      ...p,
      status: null,
      money: null,
      stock: 14,
      href: `/products/${p.id}`,
    })),
  },
}
const ACTION = (n: number | string) =>
  `0193f6b0-1c2d-7e3f-8a9b-${String(n).padStart(12, "0")}`

/**
 * Office's review of setting Raw Honey 500g's main-store total to `total` (stock was 14). Its facts
 * carry the "what changes" rows the API builds since P3-3 (warehouse · before → after).
 */
export const stockReview = (total: number, ticket = `stock-${total}`) => ({
  status: "requires_confirmation",
  data: { actionId: ACTION(total) },
  review: {
    ticket,
    operationId: "stock_set_levels",
    method: "POST",
    summary: "Set stock levels per warehouse",
    confirmationCode: "135790",
    confirmationPhrase: "Confirm action 135790",
    target: "stock set",
    details: {
      target: { product: "p-500" },
      record: { type: "Product", label: "Raw Honey 500g" },
      current: {
        kind: "stock",
        locked: {
          id: "p-500",
          product_name: "Raw Honey 500g",
          tracks_stock: true,
          slots: [
            {
              warehouse_id: "wh-main",
              warehouse_name: "Main store",
              bin_id: null,
              before: "14",
            },
          ],
        },
        changes:
          total === 14
            ? []
            : [{ field: "stock", label: "Main store", before: "14", after: String(total) }],
      },
      changes: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: total }] },
    },
    expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
  },
})

type Json = Record<string, unknown>
const body = (route: Route) => (route.request().postDataJSON() ?? {}) as Json

/**
 * Tools + confirm for the stock task. Each approve answers like Office (`{"message":"ok"}`, the
 * action id), so the receipt must read the record back. Returns the confirm bodies seen.
 */
export async function stockRoutes(page: Page, confirmAnswer?: (ticket: string) => Json) {
  const confirms: Json[] = []
  // Every proposal gets its own ticket, as Office's do: the first for a total is `stock-<total>`,
  // a later one for the same total `stock-<total>-<n>`.
  const proposed = new Map<number, number>()
  await page.route("**/api/voice/tools", async (route) => {
    const b = body(route)
    if (b.command !== "execute") return route.fallback()
    if (b.operationId === "products_list") return route.fulfill({ json: products })
    if (b.operationId === "stock_set_levels") {
      const args = b.arguments as { body: { warehouses: { stock_lvl: number }[] } }
      const total = args.body.warehouses[0]!.stock_lvl
      const n = (proposed.get(total) ?? 0) + 1
      proposed.set(total, n)
      return route.fulfill({
        json: stockReview(total, n === 1 ? `stock-${total}` : `stock-${total}-${n}`),
      })
    }
    return route.fallback()
  })
  await page.route("**/api/voice/confirm", async (route) => {
    const b = body(route)
    confirms.push(b)
    const ticket = String(b.ticket)
    const total = ticket.split("-")[1]
    return route.fulfill({
      json:
        b.decision === "approve"
          ? (confirmAnswer?.(ticket) ?? {
              status: "completed",
              actionId: ACTION(total ?? 0),
              data: { message: "ok" },
            })
          : { status: "cancelled", actionId: ACTION(total ?? 0) },
    })
  })
  return confirms
}

/** The product's stock rows after the write, as the API lists them (2-decimal strings). */
export const readBack = (level: string): Record<string, MockReply> => ({
  "GET /workspace/w1/products/p-500/stock-with-bins": {
    body: {
      data: [
        {
          id: "s1",
          warehouse: { id: "wh-main", name: "Main store" },
          bin: null,
          lot: null,
          shelf: null,
          stock_lvl: level,
          reserved: "0.00",
          minimal_stock_level: 20,
          maximum_stock_level: 0,
        },
      ],
    },
  },
})

/** Conversation up to an open review for setting the total to `total`. */
export async function proposeStock(page: Page, agent: Agent, total = 120) {
  await page.goto("/")
  await startText(page)
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: total }] },
  })
  await expect(scene(page)).toContainText("Confirm action 135790")
}

export { ACTION }
