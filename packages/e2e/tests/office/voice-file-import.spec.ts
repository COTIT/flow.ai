import { expect, expectFits, test } from "./fake-api"
import en from "../../../../apps/office/messages/en.json" with { type: "json" }

const handle = "01950000-0000-7000-8000-0000000000aa"
const batchId = "01950000-0000-7000-8000-0000000000bb"
const csv = "Product,Price\nRice,12.50\nBeans,3\nMaize,x\n"

test("a shared price list becomes one review with the rows left out listed, never sent to the model", async ({
  page,
  api: _api,
}) => {
  void _api
  let toolRequest: { contentType: string; body: string } | undefined
  let contextual = ""
  let preparation: unknown
  await page.route("**/api/voice/session", (route) => {
    if (route.request().method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })
    if (route.request().postDataJSON()?.command === "heartbeat")
      return route.fulfill({ json: { status: "active" } })
    return route.fulfill({
      json: {
        sessionId: "import-session",
        conversationId: "import-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.route("**/api/voice/attachments", (route) => {
    expect(route.request().headers()["content-type"]).toBe("text/csv")
    expect(route.request().postData()).toBe(csv)
    return route.fulfill({
      json: {
        id: handle,
        kind: "sheet",
        status: "uploaded",
        rows: 3,
        columns: ["Product", "Price"],
      },
    })
  })
  await page.route("**/api/voice/tools", (route) => {
    const contentType = route.request().headers()["content-type"] ?? ""
    if (!contentType.startsWith("multipart/"))
      return route.fulfill({ json: { status: "completed", data: [] } })
    toolRequest = { contentType, body: route.request().postData() ?? "" }
    const step = (n: number, name: string, price: string) => ({
      actionId: `01950000-0000-7000-8000-00000000000${n}`,
      operationId: "products_update",
      method: "PATCH",
      summary: `Update ${name}`,
      details: { product: name, price },
    })
    return route.fulfill({
      json: {
        status: "requires_confirmation",
        data: { batchId, rows: { from: 1, to: 3, total: 3 } },
        review: {
          kind: "batch",
          batchId,
          ticket: "private-import-ticket",
          summary: "Update 2 prices",
          confirmationCode: "531902",
          confirmationPhrase: "Confirm action 531902",
          steps: [step(1, "Rice", "12.50"), step(2, "Beans", "3")],
          details: {
            rows: { from: 1, to: 3, total: 3 },
            notIncluded: [{ line: 4, text: "Maize", reason: "unreadable" }],
            // A batch that is one part of a bigger change says how many follow (own review each).
            remaining: 12,
          },
          expiresAt: new Date(Date.now() + 120000).toISOString(),
        },
      },
    })
  })
  let cancels = 0
  await page.route("**/api/voice/confirm", (route) => {
    expect(route.request().postDataJSON()).toMatchObject({
      ticket: "private-import-ticket",
      decision: "cancel",
    })
    cancels++
    return route.fulfill({ json: { status: "cancelled" } })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      if (event.type === "conversation_initiation_client_data")
        socket.send(
          JSON.stringify({
            type: "conversation_initiation_metadata",
            conversation_initiation_metadata_event: {
              conversation_id: "import-conversation",
              agent_output_audio_format: "pcm_16000",
              user_input_audio_format: "pcm_16000",
            },
          })
        )
      if (event.type === "contextual_update") contextual += String(raw)
      if (event.type === "user_message")
        socket.send(
          JSON.stringify({
            type: "client_tool_call",
            client_tool_call: {
              tool_name: "execute_genesy_tool",
              tool_call_id: "import",
              parameters: {
                operation_id: "assistant_import_prices",
                arguments_json: JSON.stringify({
                  attachments: { sheet: [handle] },
                }),
              },
            },
          })
        )
      if (
        event.type === "client_tool_result" &&
        event.tool_call_id === "import"
      )
        preparation = JSON.parse(event.result)
    })
  })
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await page.getByLabel("Choose reference files").setInputFiles({
    name: "prices.csv",
    mimeType: "text/csv",
    buffer: Buffer.from(csv),
  })
  await page
    .getByLabel(en.voice.message)
    .fill("Update our prices from this list")
  await page.getByRole("button", { name: en.voice.send, exact: true }).click()

  const review = page.getByRole("dialog", { name: en.voice.review })
  await expect(review).toBeVisible()
  await expect(review).toContainText("1. Update product")
  await expect(review).toContainText("Rice")
  await expect(review).toContainText("12.50")
  await expect(review).toContainText("2. Update product")
  await expect(review).toContainText("Beans")
  await expect(review).toContainText("Rows 1–3 of 3")
  await expect(review).toContainText(en.voice.notIncluded.title)
  await expect(review).toContainText(
    en.voice.moreParts.replace("{count}", "12")
  )
  await expect(review).not.toContainText("Remaining")
  await expect(review).toContainText(
    `Line 4: Maize — ${en.voice.notIncluded.reasons.unreadable}`
  )
  await expect
    .poll(() => preparation)
    .toMatchObject({ status: "requires_confirmation" })
  // The sheet's own bytes go back with the import call; the model only ever saw the handle.
  expect(toolRequest!.body).toContain(`name="${handle}"`)
  expect(toolRequest!.body).toContain("Maize,x")
  expect(contextual).toContain("shared_sheets")
  expect(contextual).not.toContain("12.50")
  await expectFits(page)
  await review
    .getByRole("button", { name: en.voice.cancel, exact: true })
    .click()
  await expect(review).toHaveCount(0)
  expect(cancels).toBe(1)
})
