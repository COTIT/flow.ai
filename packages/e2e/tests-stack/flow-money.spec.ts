import type { APIRequestContext, Page } from "@playwright/test"

import { formatMoney } from "../../ui/src/lib/money"
import { formatPrice } from "../../voice/src/lib/helpers"
import { resultView } from "../../voice/src/lib/views"
import { apiAs, cred, expect, loginFlow, openShift, pairTill, test, unique, urls } from "../src/stack"
import { expectNoHorizontalScroll } from "../src/test"
import { chat, fakeAgent, startText } from "../tests/flow/voice"
import { ownProduct } from "./till"

// REAL API replies (a real till sale and shift, a real product) carry each amount's currency and
// decimals; the card Flow draws from them is real. SIMULATED only the voice transport: the
// browser's /api/voice calls are answered here with resultView() over the real reply, exactly
// what Office's server does (apps/office/lib/voice/server.ts `completed`).

type Owner = Awaited<ReturnType<typeof apiAs>>
type Reply = { data: Record<string, unknown> | Record<string, unknown>[]; meta?: unknown }

/** The API's reply as Office's voice server receives it: the whole envelope, not just `data`. */
async function raw(request: APIRequestContext, owner: Owner, path: string) {
  const res = await request.get(`${urls.api}/v1${owner.ws(path)}`, {
    headers: { Authorization: `Bearer ${owner.token}` },
  })
  expect(res.status(), path).toBe(200)
  return (await res.json()) as Reply
}

/** Flow's voice routes answer as Office would; each read returns the picture of its real reply. */
async function office(page: Page, replies: Record<string, Reply>) {
  await page.route("**/api/voice/readiness", (route) => route.fulfill({ json: { enabled: true, ready: true } }))
  await page.route("**/api/voice/session", (route) =>
    route.fulfill({
      json: {
        sessionId: "stack-session",
        conversationId: "flow-conversation",
        signedUrl: "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=stack",
      },
    })
  )
  await page.route("**/api/voice/tools", async (route) => {
    const sent = route.request().postDataJSON() as { command?: string; operationId?: string }
    const reply = sent.command === "execute" ? replies[String(sent.operationId)] : undefined
    if (!reply) return route.fulfill({ json: { status: "completed", data: [] } })
    return route.fulfill({ json: { status: "completed", data: {}, view: resultView(String(sent.operationId), reply) } })
  })
}

/** Asks one read in a text conversation and opens its card in the results. */
async function show(page: Page, operation: string) {
  const agent = await fakeAgent(page)
  await page.goto(`${urls.flow}/`)
  await startText(page)
  agent.emit({
    type: "client_tool_call",
    client_tool_call: {
      tool_name: "execute_genesy_tool",
      tool_call_id: `call-${operation}`,
      parameters: { operation_id: operation, arguments_json: "{}" },
      expects_response: true,
    },
  })
  await expect.poll(agent.results).toBe(1)
  // P5: the answer shows inline under the reply; "Show all" opens the Results screen when it is
  // bigger than the card.
  const card = page.getByRole("log").first().locator("[data-inline-result]").first()
  await expect(card).toBeVisible()
  const all = card.getByRole("button", { name: /^Show all$/ })
  if (!(await all.isVisible())) return card
  await all.click()
  return page.getByRole("region", { name: chat.results }).locator('[data-role="result"]').first()
}

const ISO = /^[A-Z]{3}$/

test("a real till sale and shift: Flow shows each amount in the currency the API sent", async ({ page, request }) => {
  const { owner, register } = await pairTill(page, request)
  await openShift(page)
  const { name } = await ownProduct(owner, register, "money")
  await page.goto(`${urls.till}/sale`)
  await page.getByLabel("Scan a barcode or type a name").fill(name)
  await page.getByRole("list", { name: "Products" }).getByRole("button", { name: new RegExp(name) }).click()
  await page.getByRole("button", { name: /^Charge/ }).click()
  const sheet = page.getByRole("dialog", { name: "Take payment" })
  await sheet.getByRole("button", { name: "Cash" }).click()
  await sheet.getByRole("button", { name: "Exact" }).click()
  await sheet.getByRole("button", { name: /^Book sale/ }).click()
  await expect(page.getByRole("dialog", { name: "Done" })).toBeVisible()

  const sales = await raw(request, owner, `/till/sales?register_id=${register.id}&page_size=5`)
  const shifts = await raw(request, owner, `/till/shifts?register_id=${register.id}&open=true&page_size=5`)
  const sale = (sales.data as Record<string, unknown>[])[0]!
  const shift = (shifts.data as Record<string, unknown>[])[0]!
  // The API now says what currency every till amount is in.
  for (const row of [sale, shift]) {
    expect(row.currency).toMatch(ISO)
    expect(Number.isSafeInteger(row.decimals)).toBe(true)
  }
  const money = (minor: unknown, row: Record<string, unknown>) =>
    formatMoney(minor as number, row.decimals as number, row.currency as string, "en")

  await office(page, { till_sales_list: sales, till_shifts_list: shifts })
  await loginFlow(page, cred("office (AFM)", "owner"))
  const sold = await show(page, "till_sales_list")
  await expect(sold).toContainText(sale.number as string)
  await expect(sold).toContainText(money(sale.total, sale))
  await expectNoHorizontalScroll(page)

  const drawer = await show(page, "till_shifts_list")
  await expect(drawer).toContainText("Open")
  await expect(drawer).toContainText(money(shift.expected, shift))
  await expectNoHorizontalScroll(page)
})

test("a product in a TZS workspace shows its price in TZS with the decimals the API sent", async ({ page, request }) => {
  const admin = await apiAs(request, cred("office (RUAHA)", "admin"))
  const { product_id: id } = await admin.post<{ product_id: string }>(admin.ws("/products"), {
    name: `e2e money ${unique()}`,
    price: "1500.00",
  })
  const reply = await raw(request, admin, `/products/${id}`)
  const product = reply.data as Record<string, unknown>
  expect(product.currency).toBe("TZS")
  expect(Number.isSafeInteger(product.decimals)).toBe(true)

  await office(page, { products_get: reply })
  await loginFlow(page, cred("office (RUAHA)", "admin"))
  const card = await show(page, "products_get")
  // The API's own price incl. VAT, in its currency, with as many places as the API says.
  const price = formatPrice(product.price_incl as string, product.decimals as number, "TZS", "en")
  expect(price).toMatch(/^TZS\s[\d,]+/)
  await expect(card).toContainText(price)
  await expectNoHorizontalScroll(page)
})
