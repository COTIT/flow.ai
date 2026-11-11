import type { Page } from "@playwright/test"

import discoveryEn from "../../../../apps/flow/messages/en/discovery.json" with { type: "json" }
import { expect } from "../../src/test"
import { AMINA, NO_ALERTS, flowApi } from "./api"
import { dock } from "./shell"

/** Flow discovery texts (pills, hint, explorer). */
export const d = discoveryEn.discovery
export const label = (id: keyof typeof d.capabilities) =>
  d.capabilities[id].label

export const OWNER = {
  ...AMINA,
  permissions: [{ workspace: "w1", permissions: ["workspace_owner"] }],
}
/** Sees products only: stock, orders, sales and the storefront are not theirs. */
export const CLERK = {
  ...AMINA,
  permissions: [{ workspace: "w1", permissions: ["product.list"] }],
}

const group = (kind: string, count: number) => ({
  kind,
  count,
  open_count: count,
  acknowledged_count: 0,
  snoozed_count: 0,
  snoozed: false,
  headline_ref: null,
  impact: {},
  items: [],
  actions: ["review"],
})
/** SAMPLE Flow noticed: 3 products below minimum. */
export const LOW_STOCK = {
  data: { ...NO_ALERTS.data, groups: [group("stock.below_minimum", 3)] },
}

export const api = (user: typeof OWNER, priorities: unknown = NO_ALERTS) =>
  flowApi({
    "GET /me": { body: { data: { user } } },
    "GET /workspace/w1/attention/priorities": { body: priorities },
  })

/** The dock's suggestion buttons, in order. */
export const pills = (page: Page) =>
  dock(page)
    .getByRole("list", { name: new RegExp(`${d.suggested}|${d.next}`) })
    .getByRole("button")

/**
 * TODO(F2 merge): remove. The scene routes are F2's; until they land, a stub page stands in so a
 * pill's navigation can be asserted without a 404.
 */
export async function stubScenes(page: Page) {
  await page.route("**/scenes/**", (route) =>
    route.fulfill({
      status: 200,
      contentType: "text/html",
      body: "<!doctype html><html lang=en><title>Scene stub</title><main>Scene stub</main></html>",
    })
  )
}

/** How often this device saw `id` used (Flow's activity store for u1 in w1). */
export const usesOf = (page: Page, id: string) =>
  page.evaluate((id) => {
    const raw = localStorage.getItem("genesy.flow.activity:u1:w1")
    return (JSON.parse(raw ?? "{}").uses?.[id]?.n as number | undefined) ?? 0
  }, id)

/** Opens the explorer `times` times, each visit counted before the next. */
export async function visitExplorer(page: Page, times: number) {
  for (let i = 0; i < times; i++) {
    const before = await usesOf(page, "explore").catch(() => 0)
    await page.goto("/explore")
    await expect.poll(() => usesOf(page, "explore")).toBeGreaterThan(before)
  }
}
