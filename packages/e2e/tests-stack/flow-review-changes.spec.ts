import { apiAs, cred, expect, loginFlow, test, unique, urls } from "../src/stack"
import { expectNoHorizontalScroll } from "../src/test"
import { scene } from "../tests/flow/task"
import { fakeAgent, startText, v } from "../tests/flow/voice"

import { fakeFlowTransport } from "./flow-voice"

// REAL API review facts (the assistant proposal for a stock write) shown in Flow; SIMULATED only
// the voice side: /api/voice/tools answers with those facts as Office passes them on
// (`details.current` = the API's `review_facts.facts`; stock facts carry no money to format).

type Warehouse = { id: string; name: string }
type Facts = { kind: string; changes: { field: string; label: string; before: string; after: string }[] }

for (const [label, viewport] of [
  ["phone 375", { width: 375, height: 812 }],
  ["desktop 1280", { width: 1280, height: 800 }],
] as const) {
  test.describe(label, () => {
    test.use({ viewport })

    test("P3-3: the API's stock facts lead the card with warehouse, 14 → 130", async ({
      page,
      request,
    }) => {
      const who = cred("office (AFM)", "owner")
      const owner = await apiAs(request, who)
      const name = `e2e honey ${unique()}`
      const { product_id: id } = await owner.post<{ product_id: string }>(owner.ws("/products"), {
        name,
        price: "4.00",
      })
      const [warehouse] = await owner.get<Warehouse[]>(owner.ws("/warehouses"))
      await owner.post(owner.ws(`/products/${id}/set-stock-levels`), {
        warehouses: [{ warehouse_id: warehouse!.id, stock_lvl: 14, reason: "e2e" }],
      })
      const body = { warehouses: [{ warehouse_id: warehouse!.id, stock_lvl: 130, reason: "e2e" }] }
      const session = await owner.post<{ id: string }>(owner.ws("/assistant/sessions"), {
        language: "en",
      })
      const action = await owner.post<{ id: string; review_facts: { facts: Facts } }>(
        owner.ws("/assistant/actions"),
        {
          session_id: session.id,
          operation_id: "stock_set_levels",
          method: "POST",
          path: `/v1${owner.ws(`/products/${id}/set-stock-levels`)}`,
          body,
        }
      )
      const facts = action.review_facts.facts
      expect(facts.changes).toEqual([
        { field: "stock", label: warehouse!.name, before: "14", after: "130" },
      ])

      await page.route("**/api/voice/readiness", (route) =>
        route.fulfill({ json: { enabled: true, ready: true } })
      )
      await fakeFlowTransport(page)
      await page.route("**/api/voice/tools", (route) => {
        const sent = route.request().postDataJSON() as { command?: string }
        if (sent.command !== "execute") return route.fulfill({ json: { status: "completed", data: [] } })
        return route.fulfill({
          json: {
            status: "requires_confirmation",
            data: { actionId: action.id },
            review: {
              ticket: `stack-review-${unique()}`,
              operationId: "stock_set_levels",
              method: "POST",
              summary: "Set stock levels per warehouse",
              confirmationCode: "112233",
              confirmationPhrase: "Confirm action 112233",
              target: "stock set",
              details: {
                target: { product: id },
                record: { type: "Product", label: name },
                current: facts,
                changes: body,
              },
              expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
            },
          },
        })
      })
      await loginFlow(page, who)
      const agent = await fakeAgent(page)
      await page.goto(`${urls.flow}/`)
      await startText(page)
      agent.call("stock_set_levels", { path: { product: id }, body })
      // P5: the review shows in place (the card in the conversation).
      const results = scene(page)
      await expect(results).toContainText(v.operationTitles.stock_set_levels)
      const diff = results.locator("[data-review-diff]")
      await expect(diff).toContainText(`${warehouse!.name} · ${v.reviewFields.stock}`)
      await expect(diff.locator("del")).toHaveText(/14/)
      await expect(diff.locator("ins")).toHaveText(/130/)
      await expect(results.getByRole("button", { name: v.approve })).toBeVisible()
      await expectNoHorizontalScroll(page)
    })
  })
}
