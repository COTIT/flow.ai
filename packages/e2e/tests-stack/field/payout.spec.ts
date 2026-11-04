import type { APIRequestContext, Page } from "@playwright/test"

import { expect, stackAllow, test, urls, workspaces } from "../../src/stack"
import { expectNoRawValues } from "../../src/test"
import { farmApi, signIn, who, type Passbook } from "./field"

// Ilundo's two officers: Salima registered the seeded households with credit, Juma did not.
const juma = who("field_officer", "juma@ruaha-demo.test")
const salima = who("field_officer", "salima@ruaha-demo.test")
const PAYOUTS = `${urls.field}/v1/workspace/${workspaces.RUAHA}/farm/payouts`

type Household = {
  members: { person_id: string }[]
  provenance: { captured_by: { id: string } | null; verified_by: { id: string } | null }
}

/** A farmer in Juma's village with TZS credit whose household Salima registered and Juma neither registered nor verified. */
async function farmerWithCredit(request: APIRequestContext) {
  const officer = await farmApi(request, juma)
  const other = (await farmApi(request, salima)).me.user.id
  const households = await officer.get<Household[]>("/farm/households?page_size=100")
  for (const h of households.data) {
    const by = [h.provenance.captured_by?.id, h.provenance.verified_by?.id]
    if (by.includes(officer.me.user.id) || h.provenance.captured_by?.id !== other) continue
    for (const m of h.members) {
      const book = (await officer.get<Passbook["data"]>(`/farm/persons/${m.person_id}/passbook?page_size=1`)) as unknown as Passbook
      const tzs = book.balances.find((b) => b.currency === "TZS")
      if (tzs && tzs.minor >= 2) return { person: m.person_id, officer }
    }
  }
  throw new Error("seed: no Ilundo farmer with TZS credit registered by Salima")
}

async function payOut(page: Page, amount: string) {
  await page.getByTestId("pay-out-TZS").click()
  await page.getByTestId("payout-amount").fill(amount)
  await page.getByTestId("payout-id-type").selectOption("nida")
  await page.getByTestId("payout-name-confirmed").check()
  await page.getByTestId("confirm-dialog-confirm").click()
}

test("an officer pays a farmer out of their credit: one payout, its debit in the passbook", async ({ page, request }) => {
  const { person, officer } = await farmerWithCredit(request)
  await signIn(page, request, juma)
  await page.goto(`${urls.field}/officer/people/${person}`)
  const paid = page.waitForResponse((r) => r.url() === PAYOUTS && r.request().method() === "POST")
  await payOut(page, "0.01")
  const { data } = (await (await paid).json()) as { data: { id: string; amount: { minor: number } } }
  await expect(page.getByTestId("payout-done")).toContainText("0.01")
  expect(data.amount.minor).toBe(1)
  await expectNoRawValues(page)

  // The API's side: this payout's own debit (other runs pay this farmer too: assert by id, not totals).
  const book = (await officer.get<Passbook["data"]>(`/farm/persons/${person}/passbook?page_size=100`)) as unknown as Passbook
  expect(book.data.filter((l) => l.payout_id === data.id).map((l) => [l.kind, l.amount.minor])).toEqual([["payout", -1]])
})

test.describe("refused payouts", () => {
  test.use({ allow: { ...stackAllow, responses: [...stackAllow.responses, { url: PAYOUTS, status: 403 }] } })

  test("above the officer limit, the officer is told ops or admin must pay, and nothing is paid", async ({ page, request }) => {
    const { person, officer } = await farmerWithCredit(request)
    await signIn(page, request, juma)
    await page.goto(`${urls.field}/officer/people/${person}`)
    const sent = page.waitForRequest((r) => r.url() === PAYOUTS && r.method() === "POST")
    await payOut(page, "100000.01")
    const { client_ref } = (await sent).postDataJSON() as { client_ref: string }
    await expect(page.getByTestId("payout-error")).toContainText("Not accepted")
    await expect(page.getByTestId("payout-error")).toContainText(/above the officer limit/i)
    await expectNoRawValues(page)
    await expect(page.getByTestId("payout-done")).toHaveCount(0)
    const payouts = await officer.get<{ client_ref: string }[]>(`/farm/payouts?person_id=${person}&page_size=100`)
    expect(payouts.data.map((p) => p.client_ref)).not.toContain(client_ref)
  })

  test("four eyes: the officer who registered the household is refused", async ({ page, request }) => {
    const { person } = await farmerWithCredit(request)
    await signIn(page, request, salima)
    await page.goto(`${urls.field}/officer/people/${person}`)
    await payOut(page, "0.01")
    // A refusal, not a failure: titled as one, with the API's reason.
    await expect(page.getByTestId("payout-error")).toContainText("Not accepted")
    await expect(page.getByTestId("payout-error")).toContainText("You registered this household")
    await expectNoRawValues(page)
    await expect(page.getByTestId("payout-done")).toHaveCount(0)
  })
})
