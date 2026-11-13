import { createRequire } from "node:module"
import { readFileSync } from "node:fs"
import type { Page } from "@playwright/test"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, markInjected, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { fakeAgent, send, startText, type Agent } from "./voice"

const require = createRequire(new URL("../../../voice/package.json", import.meta.url))
const period = { from: "2026-11-01", to: "2026-11-08" }
const views = {
  products_top_sellers: { kind: "breakdown", by: "products", ...period, presentation: "pie", currencies: [{ currency: "USD", decimals: 2, rows: [
    { label: "Wild honey", gross: 120050, orders: 4 },
    { label: "Beeswax", gross: 40000, orders: 2 },
  ] }] },
  orders_sales_daily: { kind: "sales_daily", ...period, time_zone: "UTC", currencies: [{ currency: "USD", decimals: 2, days: [
    { date: "2026-11-01", gross: 120050, net: 100000, orders: 4 },
    { date: "2026-11-02", gross: 40000, net: 35000, orders: 2 },
  ] }] },
  orders_sales_summary: { kind: "sales", ...period, source: null, rows: [{ currency: "USD", decimals: 2, gross: 160050, net: 135000, vat: 25050, refunded: 0, orders: 6 }] },
} as const

function ask(agent: Agent, op: keyof typeof views, extra: object = {}) {
  agent.emit({ type: "client_tool_call", client_tool_call: {
    tool_name: "execute_genesy_tool", tool_call_id: `report-${op}-${agent.sent.length}`,
    parameters: { operation_id: op, arguments_json: "{}", ...extra }, expects_response: true,
  } })
}

async function setup(page: Page, multiple = false) {
  await seedSession(page)
  await mockApi(page, flowApi())
  const reads: string[] = []
  await page.route("**/api/voice/tools", async (route) => {
    const body = route.request().postDataJSON()
    const view = views[body.operationId as keyof typeof views]
    if (!view) return route.fallback()
    reads.push(body.operationId)
    await new Promise((resolve) => setTimeout(resolve, 80))
    await route.fulfill({ json: { status: "completed", data: {}, view: { ...view, ...(body.presentation ? { presentation: body.presentation } : {}) }, ...(body.replaces ? { replaces: body.replaces } : {}) } })
  })
  const agent = await fakeAgent(page, (_text, live) => {
    ask(live, "products_top_sellers")
    if (multiple) {
      ask(live, "orders_sales_daily")
      ask(live, "orders_sales_summary")
    }
    setTimeout(() => live.say("Here are the requested sales results."), 600)
  })
  await page.goto("/")
  await startText(page)
  return { agent, reads }
}

test("a new chart preserves history; full-size plots respond to pointer and keyboard", async ({ page }, info) => {
  const { agent } = await setup(page)
  await send(page, "Show product sales as a pie chart")
  const log = page.getByRole("log").first()
  const cards = log.locator('[data-role="result"]')
  await expect(cards).toHaveCount(1)
  const original = cards.first()
  const pie = original.locator('[data-chart="pie"]')
  await expect(pie).toBeVisible()
  const slice = pie.locator(".recharts-sector").first()
  await expect(slice, await pie.innerHTML()).toBeAttached()
  await slice.scrollIntoViewIfNeeded({ timeout: 4000 })
  const sliceBox = await slice.boundingBox()
  // The pie center belongs to both paths; use an interior point of the first wedge.
  const position = { x: sliceBox!.width / 4, y: sliceBox!.height / 4 }
  if (info.project.use.hasTouch) await slice.tap({ position })
  else await slice.hover({ position })
  await expect(pie.getByRole("tooltip")).toContainText("Wild honey")
  await expect(pie.getByRole("tooltip")).toContainText("1,200.50")
  const box = await original.boundingBox()
  const parent = await log.boundingBox()
  expect(box!.width).toBeGreaterThan(parent!.width * 0.9)
  await page.waitForTimeout(1800)
  ask(agent, "products_top_sellers", { presentation: "bar", replaces: "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b" })
  await expect(cards).toHaveCount(2)
  await expect(original.locator('[data-chart="pie"]')).toHaveCount(1)
  await expect(cards.last().locator('[data-chart="bar"]')).toBeVisible()
  await cards.last().getByRole("application").focus()
  await page.keyboard.press("ArrowRight")
  await expect(cards.last().getByRole("tooltip")).toBeVisible()
  await expectNoHorizontalScroll(page)
  await page.screenshot({ path: info.outputPath("inline-charts.png") })
})

test("three datasets survive; selected results download as real PDF and Excel", async ({ page }, info) => {
  const { agent, reads } = await setup(page, true)
  await send(page, "Show product ranking, sales trend and sales totals")
  await expect.poll(agent.results).toBe(3)
  expect(reads).toEqual(["products_top_sellers", "orders_sales_daily", "orders_sales_summary"])
  const log = page.getByRole("log").first()
  await expect(log.locator('[data-role="result"]')).toHaveCount(3)
  await expect(page.locator("[data-report-offer]")).toHaveCount(1)
  await page.getByRole("button", { name: "Prepare report", exact: true }).click()
  const picker = page.locator("[data-report-picker]")
  await expect(picker).toBeVisible()
  await expect(picker.getByRole("checkbox")).toHaveCount(3)
  const xlsxEvent = page.waitForEvent("download")
  await picker.getByRole("button", { name: "Download Excel", exact: true }).click()
  const xlsx = await xlsxEvent
  const xlsxPath = info.outputPath("selected-results.xlsx")
  await xlsx.saveAs(xlsxPath)
  const { Workbook } = require("exceljs")
  const workbook = new Workbook()
  await workbook.xlsx.readFile(xlsxPath)
  expect(workbook.worksheets).toHaveLength(4)
  const cells = JSON.stringify(workbook.worksheets.map((sheet: { getSheetValues(): unknown }) => sheet.getSheetValues()))
  expect(cells).toContain("Wild honey")
  expect(cells).toContain("120050")
  expect(cells).toContain("135000")
  await page.route("**/report-fonts/Poppins-Regular.ttf", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1000))
    await route.continue()
  })
  const pdfEvent = page.waitForEvent("download")
  await picker.getByRole("button", { name: "Download PDF", exact: true }).click()
  const exported = page.locator("[data-report-render] svg.recharts-surface")
  await expect(exported).toHaveCount(3)
  const printSafe = await exported.evaluateAll((svgs) => svgs.every((svg) =>
    svg.getAttribute("font-family") === "Poppins" &&
    ![svg, ...svg.querySelectorAll("*")].some((el) => [...el.attributes].some((attr) => attr.value.includes("var(")))
  ))
  expect(printSafe).toBe(true)
  await expect(page.locator("[data-report-legend]")).toContainText("Wild honey")
  const pdf = await pdfEvent
  const pdfPath = info.outputPath("selected-results.pdf")
  await pdf.saveAs(pdfPath)
  const bytes = readFileSync(pdfPath)
  expect(bytes.subarray(0, 5).toString()).toBe("%PDF-")
  expect(bytes.length).toBeGreaterThan(10_000)
  await expect(picker.getByRole("alert")).toHaveCount(0)
  await page.screenshot({ path: info.outputPath("report-picker.png") })
})

test("latest preview pins chart while newer results arrive; report tool opens picker", async ({ page }) => {
  const { agent } = await setup(page)
  await send(page, "Show product ranking")
  await expect.poll(agent.results).toBe(1)
  const launcher = page.getByRole("button", { name: "Latest chart", exact: true })
  const scroller = page.locator("[data-flow-scroll]")
  const scrollBefore = await scroller.evaluate((el) => el.scrollTop)
  await launcher.click()
  const preview = page.locator("[data-chart-preview]")
  await expect(preview.locator('[data-chart="pie"]')).toBeVisible()
  ask(agent, "orders_sales_daily")
  await expect.poll(agent.results).toBe(2)
  await expect(preview.locator('[data-chart="pie"]')).toBeVisible()
  await expect(preview.locator('[data-chart="line"]')).toHaveCount(0)
  await page.keyboard.press("Escape")
  await expect(preview).toBeHidden()
  await expect(launcher).toBeFocused()
  await expect.poll(() => scroller.evaluate((el) => el.scrollTop)).toBe(scrollBefore)
  agent.emit({ type: "client_tool_call", client_tool_call: { tool_name: "open_conversation_report", tool_call_id: "open-report", parameters: {}, expects_response: true } })
  await expect.poll(() => agent.sent.find((event) => event.tool_call_id === "open-report")?.result).toBe(JSON.stringify({ status: "opened" }))
  await expect(page.locator("[data-report-picker]")).toBeVisible()
})

test("report selection spans answers without another report offer", async ({ page }) => {
  const { agent } = await setup(page)
  await send(page, "Show product sales")
  await expect.poll(agent.results).toBe(1)
  const choices = page.locator("[data-report-choice]")
  await choices.first().click()
  await expect(page.getByText("Completed", { exact: true })).toBeVisible()
  await send(page, "Show product sales again")
  await expect.poll(agent.results).toBe(2)
  await choices.last().click()
  await page.getByRole("button", { name: "Report (2)", exact: true }).click()
  const picker = page.locator("[data-report-picker]")
  await expect(picker.getByRole("checkbox", { checked: true })).toHaveCount(2)
  await page.keyboard.press("Escape")
  await expect(page.locator("[data-report-offer]")).toHaveCount(0)
})

test("export failure preserves selection and offers retry", async ({ page }) => {
  const { agent } = await setup(page)
  await send(page, "Show product sales")
  await expect.poll(agent.results).toBe(1)
  await page.getByRole("button", { name: "Add to report", exact: true }).click()
  await page.getByRole("button", { name: "Report (1)", exact: true }).click()
  const picker = page.locator("[data-report-picker]")
  markInjected(page, /\/report-fonts\//)
  await page.route("**/report-fonts/Poppins-Regular.ttf", (route) => route.fulfill({ status: 503, body: "unavailable" }))
  await picker.getByRole("button", { name: "Download PDF", exact: true }).click()
  await expect(picker.getByRole("alert")).toContainText("Your selection is kept")
  await expect(picker.getByRole("checkbox", { checked: true })).toHaveCount(1)
  await page.unroute("**/report-fonts/Poppins-Regular.ttf")
  const downloaded = page.waitForEvent("download")
  await picker.getByRole("button", { name: "Download PDF", exact: true }).click()
  await downloaded
  await expect(picker.getByRole("alert")).toHaveCount(0)
})
