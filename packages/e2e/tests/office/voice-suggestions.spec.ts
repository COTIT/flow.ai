import type { Page } from "@playwright/test"
import {
  expect,
  expectFits,
  emptyOverview,
  me,
  openMenu,
  paged,
  test,
  WS,
  type FakeApi,
} from "./fake-api"
import en from "../../../../apps/office/messages/en.json" with { type: "json" }
import nl from "../../../../apps/office/messages/nl.json" with { type: "json" }

const labels = (ids: string[], messages = en) =>
  ids.map(
    (id) =>
      (messages.voice.capabilities as Record<string, { label: string }>)[id]!
        .label
  )
const prompt = (id: string) =>
  (en.voice.capabilities as Record<string, { prompt: string }>)[id]!.prompt
const invoiceLabels = labels([
  "open_invoices",
  "email_invoice",
  "resend_invoice",
  "record_payment",
])

/** Fake agent session; records what the page sent to the agent. */
async function conversation(page: Page) {
  let emit: (event: unknown) => void = () => {}
  const sent: { type?: string }[] = []
  await page.route("**/api/voice/session", (route) => {
    const body = route.request().postDataJSON()
    if (route.request().method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })
    if (body.command === "heartbeat")
      return route.fulfill({ json: { status: "active" } })
    return route.fulfill({
      json: {
        sessionId: "tips-session",
        conversationId: "tips-conversation",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    emit = (event) => socket.send(JSON.stringify(event))
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      sent.push(event)
      if (event.type === "conversation_initiation_client_data")
        emit({
          type: "conversation_initiation_metadata",
          conversation_initiation_metadata_event: {
            conversation_id: "tips-conversation",
            agent_output_audio_format: "pcm_16000",
            user_input_audio_format: "pcm_16000",
          },
        })
    })
  })
  return {
    sent,
    call: (operation: string, args: object = {}) =>
      emit({
        type: "client_tool_call",
        client_tool_call: {
          tool_name: "execute_genesy_tool",
          tool_call_id: `call-${operation}`,
          parameters: {
            operation_id: operation,
            arguments_json: JSON.stringify(args),
          },
          expects_response: true,
        },
      }),
  }
}

const asMember = (api: FakeApi, grants: string[]) =>
  api.on("GET", "/me", () => ({
    data: {
      user: { ...me, permissions: [{ workspace: WS, permissions: grants }] },
    },
  }))

const chips = (page: Page, messages = en) =>
  page
    .getByRole("group", { name: messages.voice.suggestions, exact: true })
    .getByRole("button")

test("the start screen suggests a few things to ask; a tap fills the text box and sends nothing", async ({
  page,
  api,
}) => {
  emptyOverview(api)
  const agent = await conversation(page)
  await page.goto("/voice")
  const narrow = page.viewportSize()!.width < 640
  // The fourth chip is hidden on phones, so it leaves the accessibility tree too.
  await expect(chips(page)).toHaveCount(narrow ? 3 : 4)
  for (const chip of await chips(page).all())
    expect((await chip.boundingBox())!.height).toBeGreaterThanOrEqual(44)
  await expectFits(page)
  const first = chips(page).first()
  const label = await first.textContent()
  const id = Object.entries(en.voice.capabilities).find(
    ([, text]) => text.label === label
  )?.[0]
  expect(id).toBeTruthy()
  await first.focus()
  await page.keyboard.press("Enter")
  const box = page.getByRole("textbox", { name: en.voice.message, exact: true })
  await expect(box).toHaveValue(prompt(id!))
  await expect(box).toBeFocused()
  await expect(box).toHaveAttribute("placeholder", /^Try: “.+”$/)
  expect(agent.sent.filter((event) => event.type === "user_message")).toEqual(
    []
  )
  await expectFits(page)
  await page.getByRole("button", { name: en.voice.end, exact: true }).click()
})

test("a member sees only what their permissions allow", async ({
  page,
  api,
}) => {
  emptyOverview(api)
  asMember(api, ["order.list"])
  await conversation(page)
  await page.goto("/voice")
  await expect(chips(page)).toHaveText(
    labels(["weekly_sales", "todays_orders"])
  )
})

test("opened from the invoices page, invoice suggestions come first", async ({
  page,
  api,
}) => {
  emptyOverview(api)
  api.on("GET", "/workspace/:ws/invoices", () => paged([]))
  await conversation(page)
  await page.goto("/invoices")
  await openMenu(page)
  await page
    .getByRole("navigation", { name: "Main menu" })
    .getByRole("link", { name: en.nav.voice, exact: true })
    .click()
  await expect(page).toHaveURL(/\/voice\?from=%2Finvoices$/)
  await expect(chips(page).first()).toBeVisible()
  expect(invoiceLabels).toContain(await chips(page).first().textContent())
  expect(invoiceLabels).toContain(await chips(page).nth(1).textContent())
})

test("suggestions speak the member's language", async ({
  page,
  api,
  baseURL,
}) => {
  emptyOverview(api)
  asMember(api, ["order.list"])
  await page
    .context()
    .addCookies([{ name: "NEXT_LOCALE", value: "nl", url: baseURL! }])
  await conversation(page)
  await page.goto("/voice")
  await expect(chips(page, nl)).toHaveText(
    labels(["weekly_sales", "todays_orders"], nl)
  )
})

test("after an answer, one fitting next step is offered and fills the box", async ({
  page,
  api,
}) => {
  emptyOverview(api)
  const agent = await conversation(page)
  await page.route("**/api/voice/tools", (route) =>
    route.fulfill({
      json: { status: "completed", data: { number: "INV-0042" } },
    })
  )
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  const box = page.getByRole("textbox", { name: en.voice.message, exact: true })
  await expect(box).toBeVisible()
  agent.call("invoices_get", { path: { invoice: "0195" } })
  const next = chips(page)
  await expect(next).toHaveText(labels(["email_invoice"]))
  await next.click()
  await expect(box).toHaveValue(prompt("email_invoice"))
  expect(agent.sent.filter((event) => event.type === "user_message")).toEqual(
    []
  )
  await expectFits(page)
})

test("the assistant can point to an office page as a link row", async ({
  page,
  api,
}) => {
  emptyOverview(api)
  const agent = await conversation(page)
  await page.route("**/api/voice/tools", (route) =>
    route.fulfill({
      json: {
        status: "completed",
        data: { page: "Products" },
        links: [{ label: "Products", href: "/products" }],
        narration: "Products",
      },
    })
  )
  await page.goto("/voice")
  await page
    .getByRole("button", { name: en.voice.textInstead, exact: true })
    .click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  agent.call("office_page_open", { page: "products" })
  const link = page.getByRole("log").getByRole("link", { name: /Products/ })
  await expect(link).toHaveAttribute("href", "/products")
  await expect(link).toHaveAttribute("target", "_blank")
  const result = agent.sent.find((event) => event.type === "client_tool_result")
  expect(JSON.stringify(result)).not.toContain("/products")
  await expectFits(page)
})
