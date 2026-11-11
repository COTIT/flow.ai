import type { Page } from "@playwright/test"

import { apiAs, cred, expect, loginFlow, test, unique, urls } from "../src/stack"
import { expectNoHorizontalScroll } from "../src/test"
import { fakeAgent, startText, v } from "../tests/flow/voice"

// REAL API (stock write and the receipt's read-back), Flow, and its login; SIMULATED only the
// voice side: the browser's /api/voice calls are answered here as Office would, and the approval
// performs the exact approved request against the real API with the owner's token (Office's job).

type Warehouse = { id: string; name: string; is_store: boolean }
type Owner = Awaited<ReturnType<typeof apiAs>>

/** A product of the test's own (data rule 1) with 14 units in one warehouse. */
async function honey(owner: Owner) {
  const name = `e2e honey ${unique()}`
  const { product_id: id } = await owner.post<{ product_id: string }>(
    owner.ws("/products"),
    { name, price: "4.00" }
  )
  const [warehouse] = await owner.get<Warehouse[]>(owner.ws("/warehouses"))
  await owner.post(owner.ws(`/products/${id}/set-stock-levels`), {
    warehouses: [{ warehouse_id: warehouse!.id, stock_lvl: 14, reason: "e2e" }],
  })
  return { id, name, warehouse: warehouse! }
}

const level = async (owner: Owner, id: string, warehouse: string) => {
  const rows = await owner.get<
    { warehouse: { id: string } | null; bin: unknown; lot: unknown; stock_lvl: string }[]
  >(owner.ws(`/products/${id}/stock-with-bins`))
  return rows.find((r) => r.warehouse?.id === warehouse && !r.bin && !r.lot)?.stock_lvl
}

/** Flow's voice routes: one Office-shaped review to set the total to 150; approval writes for real. */
async function voice(
  page: Page,
  owner: Owner,
  product: { id: string; name: string; warehouse: Warehouse }
) {
  const confirms: { decision?: string }[] = []
  const body = {
    warehouses: [{ warehouse_id: product.warehouse.id, stock_lvl: 150 }],
  }
  await page.route("**/api/voice/readiness", (route) =>
    route.fulfill({ json: { enabled: true, ready: true } })
  )
  await page.route("**/api/voice/session", (route) =>
    route.fulfill({
      json: {
        sessionId: "stack-session",
        conversationId: "flow-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=stack",
      },
    })
  )
  await page.route("**/api/voice/tools", async (route) => {
    const sent = route.request().postDataJSON() as { command?: string }
    if (sent.command !== "execute")
      return route.fulfill({ json: { status: "completed", data: [] } })
    return route.fulfill({
      json: {
        status: "requires_confirmation",
        data: { actionId: "0193f6b0-1c2d-7e3f-8a9b-dddddddddddd" },
        review: {
          ticket: `stack-stock-${unique()}`,
          operationId: "stock_set_levels",
          method: "POST",
          summary: "Set stock levels per warehouse",
          confirmationCode: "112233",
          confirmationPhrase: "Confirm action 112233",
          target: "stock set",
          details: {
            target: { product: product.id },
            record: { type: "Product", label: product.name },
            current: {
              kind: "stock",
              locked: {
                id: product.id,
                product_name: product.name,
                slots: [
                  {
                    warehouse_id: product.warehouse.id,
                    warehouse_name: product.warehouse.name,
                    bin_id: null,
                    before: "14",
                  },
                ],
              },
            },
            changes: body,
          },
          expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
        },
      },
    })
  })
  await page.route("**/api/voice/confirm", async (route) => {
    const sent = route.request().postDataJSON() as { decision?: string }
    confirms.push(sent)
    if (sent.decision !== "approve")
      return route.fulfill({ json: { status: "cancelled" } })
    // What Office does once approved: the exact request, answered `{"message":"ok"}` by the API.
    const data = await owner.post(
      owner.ws(`/products/${product.id}/set-stock-levels`),
      body
    )
    return route.fulfill({
      json: {
        status: "completed",
        actionId: "0193f6b0-1c2d-7e3f-8a9b-dddddddddddd",
        data: data ?? { message: "ok" },
      },
    })
  })
  const agent = await fakeAgent(page)
  await page.goto(`${urls.flow}/`)
  await startText(page)
  agent.call("stock_set_levels", { path: { product: product.id }, body })
  await expect(page.getByRole("region", { name: "Results" })).toContainText(
    "Confirm action 112233"
  )
  return confirms
}

for (const [label, viewport] of [
  ["phone 375", { width: 375, height: 812 }],
  ["desktop 1280", { width: 1280, height: 800 }],
] as const) {
  test.describe(label, () => {
    test.use({ viewport })

    test("approving writes once; the receipt's new value is the API's own read-back", async ({
      page,
      request,
    }) => {
      const who = cred("office (AFM)", "owner")
      const owner = await apiAs(request, who)
      const product = await honey(owner)
      await loginFlow(page, who)
      await voice(page, owner, product)
      await page.getByRole("button", { name: v.approve }).click()
      const value = page.locator("[data-receipt-new]")
      await expect(value).toHaveText("150")
      await expect(page.locator("[data-receipt-row]")).toContainText("14")
      expect(await level(owner, product.id, product.warehouse.id)).toBe("150.00")
      await expectNoHorizontalScroll(page)
    })

    test("rejecting leaves the stock as it was", async ({ page, request }) => {
      const who = cred("office (AFM)", "owner")
      const owner = await apiAs(request, who)
      const product = await honey(owner)
      await loginFlow(page, who)
      const confirms = await voice(page, owner, product)
      await page.getByRole("button", { name: v.cancel, exact: true }).click()
      await expect.poll(() => confirms.length).toBe(1)
      await expect(page.locator("[data-receipt]")).toHaveCount(0)
      expect(await level(owner, product.id, product.warehouse.id)).toBe("14.00")
    })
  })
}
