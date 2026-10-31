import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import {
  DRAFT,
  LIVE,
  VOUCHER,
  currencies,
  draftSurvey,
  list,
  liveRefusal,
  liveSurvey,
  one,
  question,
  results,
  timeline,
  vouchers,
} from "./ops-surveys.api"

const SURVEYS = "/workspace/w1/farm/surveys"

type Sent = { key: string; body: unknown }

/**
 * Signed in as admin (or ops) on the whole workspace. `effects` run when a write arrives, before
 * it is answered, so the reads that follow see the API's new state. Returns every write sent.
 */
async function open(
  page: Page,
  path: string,
  { as = "admin", extra = {}, effects = {} }: {
    as?: "admin" | "ops"
    extra?: Record<string, MockReply>
    effects?: Record<string, (routes: Record<string, MockReply>) => void>
  } = {},
) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  const routes: Record<string, MockReply> = {
    "GET /farm/me": me({ name: "Amina Admin", memberships: [role(as, null)] }),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/currencies": currencies,
    [`GET ${SURVEYS}`]: list,
    [`GET ${SURVEYS}/${LIVE}`]: one(liveSurvey),
    [`GET ${SURVEYS}/${LIVE}/results`]: results,
    [`GET ${SURVEYS}/${LIVE}/vouchers`]: vouchers,
    [`GET /workspace/w1/farm/vouchers/${VOUCHER}/timeline`]: timeline,
    [`GET ${SURVEYS}/${DRAFT}`]: one(draftSurvey()),
    ...extra,
  }
  await mockApi(page, routes)
  const sent: Sent[] = []
  await page.route("**/v1/**", async (route) => {
    const request = route.request()
    const key = `${request.method()} ${new URL(request.url()).pathname.replace(/^\/v1/, "")}`
    if (request.method() !== "GET") sent.push({ key, body: request.postDataJSON() })
    effects[key]?.(routes)
    await route.fallback()
  })
  await page.goto(path)
  return sent
}

const writes = (sent: Sent[], key: string) => sent.filter((s) => s.key === key)

test("the list shows every survey with its incentive and the API's response count", async ({ page }) => {
  await open(page, "/ops/surveys")

  const rows = page.getByTestId("survey-row")
  await expect(rows).toHaveCount(2)
  await expect(rows.first()).toContainText("Harvest intentions")
  await expect(rows.first()).toContainText("TZS 5,000.00")
  await expect(rows.first().getByTestId("survey-responses")).toHaveText("3")
  await expect(rows.nth(1)).toContainText("Water use")
  await expectNoHorizontalScroll(page)
})

test("an admin creates a draft; the typed reward goes as integer minor units", async ({ page }) => {
  const sent = await open(page, "/ops/surveys", {
    extra: { [`POST ${SURVEYS}`]: { status: 201, body: { data: draftSurvey() } } },
  })

  await page.getByTestId("survey-create-open").click()
  await expect(page.getByTestId("survey-new-currency")).toHaveValue("c-tzs")
  await page.getByTestId("survey-new-title").fill("Water use")
  await page.getByTestId("survey-new-reward").fill("5,000")
  await page.getByTestId("survey-new-submit").click()

  await expect(page).toHaveURL(new RegExp(`/ops/surveys/${DRAFT}$`))
  await expect(page.getByTestId("survey-editor")).toBeVisible()
  const created = writes(sent, `POST ${SURVEYS}`)
  expect(created).toHaveLength(1)
  expect(created[0]?.body).toEqual({ title_en: "Water use", reward_minor: 500000, currency_id: "c-tzs" })
  await expectNoHorizontalScroll(page)
})

test("an admin adds a question to the draft", async ({ page }) => {
  const sent = await open(page, `/ops/surveys/${DRAFT}`, {
    extra: { [`POST ${SURVEYS}/${DRAFT}/questions`]: { status: 201, body: { data: { id: "q1" } } } },
    effects: {
      [`POST ${SURVEYS}/${DRAFT}/questions`]: (routes) => {
        routes[`GET ${SURVEYS}/${DRAFT}`] = one(draftSurvey([question]))
      },
    },
  })

  await page.getByTestId("question-add").click()
  const card = page.getByTestId("question-card-new")
  await card.getByTestId("question-prompt-en").fill("Where do you store your harvest?")
  await card.getByTestId("question-add-option").click()
  await card.getByTestId("question-add-option").click()
  await card.getByTestId("question-option-label-en").nth(0).fill("In a crib")
  await card.getByTestId("question-option-label-en").nth(1).fill("In the house")
  await card.getByTestId("question-save").click()

  await expect(page.getByTestId("question-card")).toHaveCount(1)
  await expect(page.getByTestId("question-card-new")).toHaveCount(0)
  const added = writes(sent, `POST ${SURVEYS}/${DRAFT}/questions`)
  expect(added).toHaveLength(1)
  expect(added[0]?.body).toEqual({
    position: 1,
    kind: "single_choice",
    prompt_en: "Where do you store your harvest?",
    prompt_sw: null,
    required: true,
    options: [
      { value: "in_a_crib", label_en: "In a crib" },
      { value: "in_the_house", label_en: "In the house" },
    ],
  })
  await expectNoHorizontalScroll(page)
})

test("publishing goes through the dialog: cancel sends nothing, confirm sends one", async ({ page }) => {
  const publish = `POST ${SURVEYS}/${DRAFT}/publish`
  const sent = await open(page, `/ops/surveys/${DRAFT}`, {
    extra: {
      [`GET ${SURVEYS}/${DRAFT}`]: one(draftSurvey([question])),
      [publish]: one({ ...draftSurvey([question]), status: "live" }),
    },
    effects: {
      [publish]: (routes) => {
        routes[`GET ${SURVEYS}/${DRAFT}`] = one({ ...draftSurvey([question]), status: "live" })
        routes[`GET ${SURVEYS}/${DRAFT}/results`] = results
        routes[`GET ${SURVEYS}/${DRAFT}/vouchers`] = vouchers
      },
    },
  })

  await page.getByTestId("survey-publish").click()
  await expect(page.getByTestId("confirm-dialog")).toBeVisible()
  await page.getByTestId("confirm-dialog-cancel").click()
  await expect(page.getByTestId("confirm-dialog")).toHaveCount(0)
  expect(writes(sent, publish)).toHaveLength(0)

  await page.getByTestId("survey-publish").click()
  await page.getByTestId("confirm-dialog-confirm").click()
  await expect(page.getByTestId("survey-results")).toBeVisible()
  await expect(page.getByTestId("survey-publish")).toHaveCount(0)
  expect(writes(sent, publish)).toHaveLength(1)
  await expectNoHorizontalScroll(page)
})

test("results show the counts and sums the API sent", async ({ page }) => {
  await open(page, `/ops/surveys/${LIVE}`)

  await expect(page.getByTestId("summary-responses")).toHaveText("3")
  await expect(page.getByTestId("summary-issued")).toHaveText("3 · TZS 15,000.00")
  await expect(page.getByTestId("summary-redeemed")).toHaveText("1 · TZS 5,000.00")
  await expect(page.getByTestId("summary-outstanding")).toHaveText("1 · TZS 5,000.00")
  await expect(page.getByTestId("summary-void")).toHaveText("1")
  const option = page.getByTestId("tally-option").first()
  await expect(option).toContainText("In a crib")
  await expect(option).toContainText("2")
  await expect(page.getByTestId("survey-published-by")).toContainText("Amina Admin")
  await expectNoHorizontalScroll(page)
})

test("voiding a voucher goes through the dialog with the reason", async ({ page }) => {
  const voiding = `POST /workspace/w1/farm/vouchers/${VOUCHER}/void`
  const sent = await open(page, `/ops/surveys/${LIVE}`, { extra: { [voiding]: { body: {} } } })

  await page.getByTestId("voucher-row").click()
  await page.getByTestId("voucher-void-reason").fill("Household answered twice")
  await page.getByTestId("voucher-void").click()
  await page.getByTestId("confirm-dialog-cancel").click()
  await expect(page.getByTestId("confirm-dialog")).toHaveCount(0)
  expect(writes(sent, voiding)).toHaveLength(0)

  await page.getByTestId("voucher-void").click()
  await page.getByTestId("confirm-dialog-confirm").click()
  await expect(page.getByTestId("confirm-dialog")).toHaveCount(0)
  const voided = writes(sent, voiding)
  expect(voided).toHaveLength(1)
  expect(voided[0]?.body).toEqual({ reason: "Household answered twice" })
  await expectNoHorizontalScroll(page)
})

test.describe("a refused edit", () => {
  test.use({ allow: { responses: [{ url: `/v1${SURVEYS}/${DRAFT}`, status: 422 }] } })

  test("shows the API's message as written", async ({ page }) => {
    await open(page, `/ops/surveys/${DRAFT}`, { extra: { [`PATCH ${SURVEYS}/${DRAFT}`]: liveRefusal } })

    await page.getByTestId("survey-title-en").fill("Water use 2026")
    await page.getByTestId("survey-save").click()
    await expect(page.getByTestId("survey-editor").getByTestId("error-state")).toContainText(
      "a live survey cannot be edited",
    )
    await expectNoHorizontalScroll(page)
  })
})

test("ops read surveys with no authoring controls", async ({ page }) => {
  await open(page, "/ops/surveys", { as: "ops" })
  await expect(page.getByTestId("survey-row")).toHaveCount(2)
  await expect(page.getByTestId("survey-create-open")).toHaveCount(0)

  await page.getByTestId("survey-row").nth(1).click()
  await expect(page.getByText("Only an admin can author surveys.")).toBeVisible()
  await expect(page.getByTestId("survey-editor")).toHaveCount(0)
  await expect(page.getByTestId("survey-publish")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})
