import type { Page, Route } from "@playwright/test"

import taskEn from "../../../../apps/flow/messages/en/task.json" with { type: "json" }
import type { MockReply } from "../../src/api-mock"
import { expect } from "../../src/test"
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
export const scene = (page: Page) =>
  page.getByRole("region", { name: chat.results })

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

/** Office's review of setting Raw Honey 500g's main-store total to `total` (stock was 14). */
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
  await page.route("**/api/voice/tools", async (route) => {
    const b = body(route)
    if (b.command !== "execute") return route.fallback()
    if (b.operationId === "products_list") return route.fulfill({ json: products })
    if (b.operationId === "stock_set_levels") {
      const args = b.arguments as { body: { warehouses: { stock_lvl: number }[] } }
      return route.fulfill({ json: stockReview(args.body.warehouses[0]!.stock_lvl) })
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
