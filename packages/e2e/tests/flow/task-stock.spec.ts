import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, ILUNDO, mockApi, RUAHA } from "./api"
import { orbAt, revealHeader } from "./shell"
import {
  capsule,
  k,
  proposeStock,
  readBack,
  receipt,
  scene,
  stockRoutes,
} from "./task"
import { fakeAgent, send, startText, v, watch } from "./voice"

// SIMULATED voice (fake ElevenLabs socket + Office-shaped answers); the read-back is the mocked API.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi(readBack("150.00")))
})

test("A: ambiguous honey → choose 500g → set 120 → 'actually 150' → only 150 runs → receipt 14 → 150", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  const confirms = await stockRoutes(page)
  const sessions = watch(page, "session")
  await page.goto("/")
  await startText(page)
  await send(page, "Update the honey stock.")
  agent.call("products_list", { query: { q: "honey" } })
  await expect.poll(agent.results).toBe(1)
  agent.say("I found three honey products. Which one did you mean?")

  // The question becomes a choice: the task waits for input, reachable from the dock.
  await expect(capsule(page)).toHaveAttribute("data-task-state", "waiting_input")
  await capsule(page).getByRole("button", { name: k.review }).click()
  const choice = page.getByRole("form", { name: k.clarify.title })
  await expect(choice.getByRole("radio")).toHaveCount(3)
  await expectNoHorizontalScroll(page)
  await choice.getByRole("radio", { name: /Raw Honey 500g/ }).check()
  await choice.getByLabel(k.clarify.type).selectOption({ label: k.clarify.types.set })
  // A quantity is required for a stock change; ids are never typed.
  await expect(choice.getByRole("button", { name: k.clarify.prepare })).toBeDisabled()
  await choice.getByLabel(k.clarify.quantity).fill("120")
  await choice.getByRole("button", { name: k.clarify.prepare }).click()

  // One plain sentence to Genesy; the context frame carries only the record id (test G).
  await expect
    .poll(() =>
      agent.sent.some(
        (e) =>
          e.type === "user_message" &&
          e.text === "Set the stock of Raw Honey 500g (HNY-500) to a total of 120 units."
      )
    )
    .toBe(true)
  await expect
    .poll(() =>
      agent.sent
        .filter((e) => e.type === "contextual_update")
        .some((e) => String(e.text).includes("p-500"))
    )
    .toBe(true)
  expect(
    agent.sent
      .filter((e) => e.type === "contextual_update")
      .some((e) => String(e.text).includes("Raw Honey"))
  ).toBe(false)

  // Genesy proposes 120: the approval scene, orb compact, same connection (design test 4).
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })
  await expect(scene(page)).toContainText("Confirm action 135790")
  await orbAt(page, "compact")

  // A spoken correction cancels the 120 proposal: withdrawn, never sent.
  agent.emit({
    type: "user_transcript",
    user_transcription_event: { user_transcript: "Actually, make that 150.", event_id: 7 },
  })
  await expect.poll(() => confirms.length).toBe(1)
  expect(confirms[0]).toMatchObject({ decision: "cancel", ticket: "stock-120" })
  await expect(page.locator("[data-withdrawn]")).toContainText(k.withdrawn)
  await expect(page.locator("[data-withdrawn]")).toContainText("Main store → 120")
  // Genesy re-proposes once the cancel is through: a new review, a new ticket.
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 150 }] },
  })
  await expect(capsule(page)).toHaveAttribute("data-task-state", "needs_approval")

  // Only the 150 proposal is approved (design test 5: success without a reset).
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
  expect(confirms.filter((c) => c.decision === "approve")).toEqual([
    expect.objectContaining({ ticket: "stock-150" }),
  ])
  // Previous from the review snapshot, new from the read after the write.
  await expect(receipt(page).locator("[data-receipt-row]")).toContainText("14")
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("150")
  await expect(receipt(page)).toContainText(k.receipt.statuses.completed)
  await expect(receipt(page)).toContainText("0193f6b0-1c2d-7e3f-8a9b-000000000150")
  await expectNoHorizontalScroll(page)
  expect(agent.connections).toBe(1)
  expect(sessions.filter((s) => s.method === "POST").length).toBe(1)
})

test("rejecting the proposal leaves stock unchanged and reads nothing back", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  const confirms = await stockRoutes(page)
  const reads: string[] = []
  page.on("request", (r) => {
    if (r.url().includes("/stock-with-bins")) reads.push(r.url())
  })
  await page.goto("/")
  await startText(page)
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })
  await scene(page).getByRole("button", { name: v.cancel, exact: true }).click()
  await expect.poll(() => confirms.length).toBe(1)
  expect(confirms[0]).toMatchObject({ decision: "cancel" })
  await expect(receipt(page)).toHaveCount(0)
  expect(reads).toHaveLength(0)
})

test("a voice answer while the chooser is open wins; the chooser is gone, never used twice", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  agent.call("products_list", { query: { q: "honey" } })
  await expect.poll(agent.results).toBe(1)
  agent.say("Which one did you mean?")
  await capsule(page).getByRole("button", { name: k.review }).click()
  const choice = page.getByRole("form", { name: k.clarify.title })
  // A pick is under way on screen...
  await choice.getByRole("radio", { name: /Raw Honey 500g/ }).check()
  // ...when the person says another one: the spoken answer is the selection.
  agent.emit({
    type: "user_transcript",
    user_transcription_event: { user_transcript: "The 1kg one.", event_id: 9 },
  })
  await expect(choice).toHaveCount(0)
  await expect(page.getByRole("radio")).toHaveCount(0)
  // Nothing from the stale chooser was sent: no message, no selected record in the context.
  const sent = () =>
    agent.sent.filter(
      (e) => e.type === "user_message" && String(e.text).includes("Raw Honey")
    )
  expect(sent()).toHaveLength(0)
  expect(
    agent.sent
      .filter((e) => e.type === "contextual_update")
      .some((e) => String(e.text).includes("p-500"))
  ).toBe(false)
  await expect(capsule(page)).toHaveCount(0)
})

test("a task belongs to its workspace: after switching, nothing of it shows, also after switching back", async ({
  page,
}) => {
  await seedSession(page, { workspaceIds: ["w1", "w2"] })
  await mockApi(
    page,
    flowApi({
      ...readBack("120.00"),
      "GET /me/workspaces": { body: { data: [ILUNDO, RUAHA] } },
    })
  )
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await proposeStock(page, agent, 120)
  // In workspace w1: an approved change (its receipt is kept) ...
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("120")
  // ... and a proposal withdrawn by a correction, shown above the next one.
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 130 }] },
  })
  await expect(scene(page).getByRole("button", { name: v.approve })).toBeVisible()
  agent.emit({
    type: "user_transcript",
    user_transcription_event: { user_transcript: "No, 125.", event_id: 5 },
  })
  await expect(scene(page).getByRole("button", { name: v.approve })).toHaveCount(0)
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 125 }] },
  })
  await expect(page.locator("[data-withdrawn]")).toContainText("→ 130")

  const picker = page.getByRole("combobox", { name: "Workspace" })
  for (const ws of [RUAHA, ILUNDO]) {
    await revealHeader(page)
    await picker.click()
    await page.getByRole("option", { name: ws.display_name, exact: true }).click()
    await expect(
      page.getByRole("button", { name: v.textInstead, exact: true })
    ).toBeVisible()
    await expect(capsule(page)).toHaveCount(0)
    await expect(receipt(page)).toHaveCount(0)
    await expect(page.locator("[data-withdrawn]")).toHaveCount(0)
  }
})
