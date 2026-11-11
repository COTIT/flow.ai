// Persona 6, the flaky-network commuter: the train goes through tunnels. Offline and online at
// random moments, slow 3G, requests dropped mid-flight, answers late or out of order, reloads
// while waiting, the voice socket lost mid-turn. SIMULATED API and voice (see harness.ts).
// Here: only P6-3 (the rest of this persona lives on its own branch).

import { markInjected } from "../../../src/test"
import { readBack, scene, stockRoutes } from "../task"
import { startText, v } from "../voice"
import { expect, test } from "./harness"



test.describe("P6-3: reloading while an approval is on its way loses the task", () => {
  for (const officeSays of ["running", "completed", "unknown"] as const)
    test(`P6-3: Office says ${officeSays}; the card or its state must come back`, async ({ persona, page, api }) => {
      const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
      api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
      const confirms = await stockRoutes(page)
      await page.route(/\/api\/voice\/tools/, (route) => {
        const b = route.request().postDataJSON() as { command?: string }
        if (b.command !== "status") return route.fallback()
        return route.fulfill({
          json: {
            status: "completed",
            data: [{ status: officeSays, data: { actionId: "0193f6b0-1c2d-7e3f-8a9b-000000000120", operation: "stock_set_levels", summary: "Set stock levels per warehouse" } }],
          },
        })
      })
      await page.goto("/")
      await startText(page)
      persona.agent.call("stock_set_levels", {
        path: { product: "p-500" },
        body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
      })
      await expect(scene(page)).toContainText("Confirm action 135790")
      // The approval leaves the page and is held 3 s; a reload then cancels the held request in
      // this browser, so "sent" counts it at the moment it left (Office would have it).
      const sent: string[] = []
      await page.route(/\/api\/voice\/confirm/, async (route) => {
        sent.push(route.request().url())
        await new Promise((r) => setTimeout(r, 3000))
        await route.fallback().catch(() => {})
      })
      markInjected(page, /\/api\/voice\/confirm/)
      await scene(page).getByRole("button", { name: v.approve }).click()
      // The approval is on the wire (Office will get it), the answer not back yet: the person reloads.
      await page.waitForTimeout(300)
      expect(confirms).toHaveLength(0)
      await persona.reload()
      // The person must learn what happened to the change they approved (or that it is being checked).
      await expect(page.locator("body")).toContainText(/Set stock levels|Raw Honey 500g/, { timeout: 8_000 })
      // Office's answer decides what the card says: confirmed only when it completed.
      await page.goto("/?view=task")
      await expect(page.locator("[data-receipt]")).toHaveAttribute(
        "data-receipt",
        officeSays === "completed" ? "completed" : "unknown"
      )
      await persona.check()
      // The route stays across the reload, so a second send would be counted here too.
      expect(sent, "the approval left once, never again after the reload").toHaveLength(1)
      expect(confirms.length, "Office got it at most once").toBeLessThanOrEqual(1)
    })
})
