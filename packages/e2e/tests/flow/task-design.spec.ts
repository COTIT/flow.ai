import type { Page, Request } from "@playwright/test"

import flowEn from "../../../../apps/flow/messages/en.json" with { type: "json" }
import { expectNoHorizontalScroll } from "../../src/test"
import {
  designEdits,
  designs,
  draft,
  expect,
  previewFrame,
  proposalFrame,
  published,
  test,
} from "./storefront-api"
import type { FakeApi } from "../office/fake-api"
import { chat, fakeAgent, startText, v, watch } from "./voice"
import { k, receipt } from "./task"

// SIMULATED voice (fake ElevenLabs socket + mock upstream) and a faked API: a storefront design
// review in the conversation, with the page it leads to shown before approval.

const s = flowEn.flow.storefront
const PHRASE = "Confirm action 246802"
const DRY_RUN = "/v1/workspace/w1/storefront/pages/home/edits/preview"

const edit = (heading = "Fresh this week") => ({
  body: {
    expected_version: 2,
    base_revision_id: draft,
    changes: [
      { op: "set_theme", theme: "neutral" },
      { op: "set_product_grid", section_id: "featured-products", heading },
    ],
  },
})

/** Every request a storefront preview frame makes: it may only load its own page (GET). */
function frameRequests(page: Page) {
  const seen: Request[] = []
  page.on("request", (request) => {
    const frame = request.frame()
    if (
      frame !== page.mainFrame() &&
      frame.url().includes("/storefront/preview")
    )
      seen.push(request)
  })
  return seen
}

/** The agent proposes one design operation; returns the review and where its buttons are. */
async function propose(page: Page, operation: string, args: unknown) {
  const agent = await fakeAgent(page)
  const confirms = watch(page, "confirm")
  await page.goto("/")
  await startText(page)
  agent.call(operation, args)
  // The review opens the result scene by itself, buttons included.
  const review = page.getByRole("region", { name: chat.results })
  await expect(
    review.getByText(v.review, { exact: false }).first()
  ).toBeVisible()
  return { agent, confirms, review, actions: review }
}

/** The whole edit review: diff, proposal frame at every device size, approve once. */
async function editReview(page: Page, api: FakeApi) {
  designs(api)
  const sent = designEdits(api)
  const fromFrame = frameRequests(page)
  const { confirms, review, actions } = await propose(
    page,
    "storefront_design_edit",
    edit()
  )

  // The diff names the section and the field; the dry run got the exact request body.
  // The section's name, and the field's once voice.reviewFields names it (P3-B adds "heading").
  const field = (v.reviewFields as Record<string, string>).heading
  await expect(review).toContainText(
    field ? `Fresh from our growers · ${field}` : "Fresh from our growers"
  )
  await expect(review).toContainText("Fresh this week")
  await expect(review).toContainText(PHRASE)
  await expect.poll(() => sent.length).toBe(1)
  expect(sent[0]).toEqual(edit().body)

  const region = review.getByRole("region", { name: s.editPreviewTitle })
  await expect(region).toContainText(s.editPreviewBadge)
  const frame = await proposalFrame(page)
  await expect(
    frame.getByRole("heading", { name: "Fresh this week" })
  ).toBeVisible()
  await expectNoHorizontalScroll(page)

  // Every device size renders the same proposal.
  for (const size of [s.tablet, s.desktop, s.mobile]) {
    await region.getByRole("button", { name: size }).click()
    await expect(region.getByRole("button", { name: size })).toHaveAttribute(
      "aria-pressed",
      "true"
    )
    await expect(
      frame.getByRole("heading", { name: "Fresh this week" })
    ).toBeVisible()
  }

  // The first proposal wins: a later message, even from this page, changes nothing.
  await page.evaluate(() => {
    const target = document.querySelector<HTMLIFrameElement>(
      'iframe[title="Proposed shop preview"]'
    )!
    target.contentWindow!.postMessage(
      {
        type: "genesy:storefront-proposal",
        document: { sections: [] },
        collections: [],
      },
      location.origin
    )
  })
  await expect(
    frame.getByRole("heading", { name: "Fresh this week" })
  ).toBeVisible()

  await actions.getByRole("button", { name: v.approve }).dblclick()
  await expect
    .poll(() => confirms.filter((c) => c.method === "POST").length)
    .toBe(1)
  await expect(
    page.getByRole("link", { name: /Storefront design/ })
  ).toBeVisible()
  // The frame fetched nothing: no API call, no write; only its own page, by GET.
  expect(fromFrame.filter((r) => r.method() !== "GET")).toEqual([])
  expect(fromFrame.filter((r) => r.url().includes("/v1/"))).toEqual([])
}

test("an edit review shows each change and the proposed page; approving runs it once", async ({
  page,
  api,
}) => {
  await editReview(page, api)
})

test.describe("on a tablet (834 px)", () => {
  test.use({ viewport: { width: 834, height: 1194 } })

  test("the edit review opens in the result scene with the proposed page", async ({
    page,
    api,
  }) => {
    await editReview(page, api)
  })
})

test("while the dry run runs, approval is held on the button and by voice", async ({
  page,
  api,
}) => {
  designs(api)
  designEdits(api)
  let release = () => {}
  const gate = new Promise<void>((done) => (release = done))
  await page.route("**/edits/preview", async (route) => {
    await gate
    await route.fallback()
  })
  const { agent, confirms, review, actions } = await propose(
    page,
    "storefront_design_edit",
    edit()
  )
  await expect(actions.getByRole("button", { name: v.approve })).toBeDisabled()
  await expect(review).not.toContainText(PHRASE)
  agent.emit({
    type: "user_transcript",
    user_transcription_event: { user_transcript: PHRASE, event_id: 42 },
  })
  await page.waitForTimeout(500)
  expect(confirms.filter((c) => c.method === "POST")).toEqual([])

  release()
  await proposalFrame(page)
  await expect(actions.getByRole("button", { name: v.approve })).toBeEnabled()
  await expect(review).toContainText(PHRASE)
})

test("rejecting sends a cancel and writes nothing", async ({ page, api }) => {
  designs(api)
  designEdits(api)
  const { confirms, actions } = await propose(
    page,
    "storefront_design_edit",
    edit()
  )
  await proposalFrame(page)
  await actions.getByRole("button", { name: v.cancel }).click()
  await expect
    .poll(() => confirms.map((c) => (c.body as { decision?: string }).decision))
    .toEqual(["cancel"])
  await expect(
    page.locator('iframe[title="Proposed shop preview"]')
  ).toHaveCount(0)
})

test.describe("the draft moved after the review was prepared", () => {
  test.use({ allow: { responses: [{ url: DRY_RUN, status: 409 }] } })

  test("no preview, and approval is held on the button and by voice; cancel still works", async ({
    page,
    api,
  }) => {
    designs(api)
    designEdits(api, "conflict")
    const { agent, confirms, review, actions } = await propose(
      page,
      "storefront_design_edit",
      edit()
    )
    await expect(review).toContainText(s.proposalConflict)
    await expect(review).toContainText(s.proposalConflictText)
    await expect(page.locator("iframe")).toHaveCount(0)
    await expect(
      actions.getByRole("button", { name: v.approve })
    ).toBeDisabled()
    await expect(review).not.toContainText(PHRASE)
    await expectNoHorizontalScroll(page)

    // The exact phrase, spoken: nothing is sent and the review stays.
    agent.emit({
      type: "user_transcript",
      user_transcription_event: { user_transcript: PHRASE, event_id: 41 },
    })
    await page.waitForTimeout(500)
    expect(confirms.filter((c) => c.method === "POST")).toEqual([])
    await expect(review).toContainText(s.proposalConflict)

    await actions.getByRole("button", { name: v.cancel }).click()
    await expect
      .poll(() =>
        confirms.map((c) => (c.body as { decision?: string }).decision)
      )
      .toEqual(["cancel"])
  })
})

test.describe("the dry run fails", () => {
  test.use({ allow: { responses: [{ url: DRY_RUN, status: 500 }] } })

  test("says the preview is unavailable, offers to try again and keeps approval", async ({
    page,
    api,
  }) => {
    designs(api)
    const sent = designEdits(api, "broken")
    const { review, actions } = await propose(
      page,
      "storefront_design_edit",
      edit()
    )
    await expect(review).toContainText(s.unavailable)
    await expect(review).toContainText(s.proposalUnavailableText)
    await review.getByRole("button", { name: s.retry }).click()
    await expect.poll(() => sent.length).toBe(2)
    await expect(actions.getByRole("button", { name: v.approve })).toBeEnabled()
    await expect(review).toContainText(PHRASE)
  })
})

test("an approval that meets a moved draft is refused in plain words", async ({
  page,
  api,
}) => {
  designs(api)
  designEdits(api)
  const { actions } = await propose(
    page,
    "storefront_design_edit",
    edit("Raced")
  )
  await proposalFrame(page)
  await actions.getByRole("button", { name: v.approve }).click()
  // The approval turns into a "not done" receipt; no success and no record link.
  await expect(receipt(page)).toHaveAttribute("data-receipt", "failed")
  await expect(receipt(page)).toContainText(k.receipt.title.failed)
  await expect(page.getByText(k.receipt.title.completed)).toHaveCount(0)
  await expect(
    page.getByRole("link", { name: /Storefront design/ })
  ).toHaveCount(0)
})

test.describe("the approval's answer is lost", () => {
  test.use({ allow: { failures: ["/api/voice/confirm"] } })

  test("says the outcome needs checking, not that it worked", async ({
    page,
    api,
  }) => {
    designs(api)
    designEdits(api)
    await page.route("**/api/voice/confirm", (route) => route.abort("failed"))
    const { actions } = await propose(page, "storefront_design_edit", edit())
    await proposalFrame(page)
    await actions.getByRole("button", { name: v.approve }).click()
    await expect(receipt(page)).toHaveAttribute("data-receipt", "unknown")
    await expect(receipt(page)).toContainText(k.receipt.title.unknown)
    await expect(
      page.getByRole("link", { name: /Storefront design/ })
    ).toHaveCount(0)
  })
})

test("a publish review warns, and shows exactly the revision that will be published", async ({
  page,
  api,
}) => {
  designs(api)
  const { review } = await propose(page, "storefront_design_publish", {
    body: { expected_version: 2, revision_id: draft },
  })
  await expect(review).toContainText("This replaces the design customers see.")
  await expect(
    review.getByRole("region", { name: s.publishPreviewTitle })
  ).toContainText(s.publishPreviewBadge)
  const frame = await previewFrame(page)
  expect(frame.url()).toContain(`revision=${draft}`)
  await expect(
    frame.getByRole("heading", { name: "Fresh from our growers" })
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("a restore review shows the revision that becomes the draft; keyboard switches sizes", async ({
  page,
  api,
}) => {
  designs(api)
  const { review } = await propose(page, "storefront_design_restore", {
    body: { expected_version: 2, revision_id: published },
  })
  const region = review.getByRole("region", { name: s.restorePreviewTitle })
  await expect(region).toContainText(s.restorePreviewText)
  const frame = await previewFrame(page)
  expect(frame.url()).toContain(`revision=${published}`)
  await region.getByRole("button", { name: s.tablet }).focus()
  await page.keyboard.press("Enter")
  await expect(region.getByRole("button", { name: s.tablet })).toHaveAttribute(
    "aria-pressed",
    "true"
  )
})
