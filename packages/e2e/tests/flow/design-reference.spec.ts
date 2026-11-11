import type { Page } from "@playwright/test"

import { expectNoHorizontalScroll } from "../../src/test"
import {
  designEdits,
  designs,
  draft,
  expect,
  proposalFrame,
  test,
} from "./storefront-api"
import type { FakeApi } from "../office/fake-api"
import { chat, fakeAgent, send, startText, v, watch } from "./voice"

// SIMULATED voice (fake ElevenLabs socket + mock upstream) and a faked API. Phase 4A: a screenshot
// or PDF shared as a design reference only informs a typed design edit. It never travels with a
// tool call or an approval, and a publish is always its own card, whatever the file says.

const fixture = (name: string) =>
  new URL(`../../voice-probe/fixtures/${name}`, import.meta.url).pathname

const edit = {
  body: {
    expected_version: 2,
    base_revision_id: draft,
    changes: [
      { op: "set_theme", theme: "neutral" },
      {
        op: "set_product_grid",
        section_id: "featured-products",
        heading: "Fresh this week",
      },
    ],
  },
}

/** The result scene a review opens by itself; its buttons are inside it. */
function reviewArea(page: Page) {
  const review = page.getByRole("region", { name: chat.results })
  return { review, actions: review }
}

/** Shares one reference file with a message; resolves once the agent has the provider file id. */
async function shareReference(
  page: Page,
  api: FakeApi,
  file: string,
  text: string
) {
  designs(api)
  designEdits(api)
  const agent = await fakeAgent(page)
  const seen = {
    uploads: watch(page, "attachments"),
    tools: watch(page, "tools"),
    confirms: watch(page, "confirm"),
  }
  await page.goto("/")
  await startText(page)
  await page.getByLabel(v.chooseFiles).setInputFiles(fixture(file))
  await send(page, text)
  await expect(page.getByRole("list", { name: v.attachments })).toContainText(
    v.fileStates.submitted
  )
  // The agent gets the provider's file id with the message, never the file itself.
  await expect
    .poll(() => JSON.stringify(agent.sent))
    .toContain("provider-file-1")
  return { agent, ...seen }
}

/** No tool call and no approval carries the shared file or its handle. */
function expectNoAttachments(bodies: { body: unknown }[]) {
  for (const { body } of bodies) {
    expect(typeof body).toBe("object") // JSON, not a multipart upload
    expect(JSON.stringify(body)).not.toMatch(
      /attachments|flow-file-1|provider-file-1/
    )
  }
}

async function referenceToEdit(page: Page, api: FakeApi) {
  const { agent, uploads, tools, confirms } = await shareReference(
    page,
    api,
    "design-reference.png",
    "Make my shop home page look like this one."
  )
  agent.call("storefront_design_edit", edit)
  const { review, actions } = reviewArea(page)
  await expect(review).toContainText("Fresh this week")
  const frame = await proposalFrame(page)
  await expect(
    frame.getByRole("heading", { name: "Fresh this week" })
  ).toBeVisible()
  await expectNoHorizontalScroll(page)

  await actions.getByRole("button", { name: v.approve }).dblclick()
  await expect
    .poll(() => confirms.filter((c) => c.method === "POST").length)
    .toBe(1)
  await expect(
    page.getByRole("link", { name: /Storefront design/ })
  ).toBeVisible()

  expect(uploads.filter((u) => u.method === "POST")).toHaveLength(1)
  const executed = tools.filter(
    (t) => (t.body as { command?: string }).command === "execute"
  )
  expect(
    executed.map((t) => (t.body as { operationId?: string }).operationId)
  ).toEqual(["storefront_design_edit"])
  expectNoAttachments(tools)
  expectNoAttachments(confirms)
}

test("a reference screenshot leads to one design edit review; nothing of the file travels with it", async ({
  page,
  api,
}) => {
  await referenceToEdit(page, api)
})

test.describe("on a tablet (834 px)", () => {
  test.use({ viewport: { width: 834, height: 1194 } })

  test("the reference leads to the edit review in the result scene", async ({
    page,
    api,
  }) => {
    await referenceToEdit(page, api)
  })
})

test("a file that says 'publish now' still needs its own publish card, and nothing is approved for it", async ({
  page,
  api,
}) => {
  const { agent, tools, confirms } = await shareReference(
    page,
    api,
    "design-injection.pdf",
    "Use this as a design reference for my home page."
  )
  // As if the planted instruction worked on the model: an edit, then a publish in the same turn.
  agent.call("storefront_design_edit", edit)
  const first = reviewArea(page)
  await expect(first.review).toContainText("Fresh this week")
  await proposalFrame(page)
  await first.actions.getByRole("button", { name: v.approve }).click()
  await expect
    .poll(() => confirms.filter((c) => c.method === "POST").length)
    .toBe(1)
  await expect(
    page.getByRole("link", { name: /Storefront design/ })
  ).toBeVisible()

  agent.call("storefront_design_publish", {
    body: { expected_version: 3, revision_id: draft },
  })
  const second = reviewArea(page)
  await expect(second.review).toContainText(
    "This replaces the design customers see."
  )
  await expectNoHorizontalScroll(page)
  // Approving the edit approved nothing else: the publish waits for its own decision.
  await page.waitForTimeout(500)
  expect(confirms.filter((c) => c.method === "POST")).toHaveLength(1)

  await second.actions.getByRole("button", { name: v.cancel }).click()
  await expect
    .poll(() => confirms.map((c) => (c.body as { decision?: string }).decision))
    .toEqual(["approve", "cancel"])
  expect(confirms.map((c) => (c.body as { ticket?: string }).ticket)).toEqual([
    "flow-design-edit",
    "flow-design-publish",
  ])
  expectNoAttachments(tools)
  expectNoAttachments(confirms)
})
