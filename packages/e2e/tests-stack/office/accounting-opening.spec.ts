import type { APIRequestContext } from "@playwright/test"

import { command, freshWorkspace, readyBook } from "../../src/accounting"
import { psql } from "../../src/erp"
import { type Api, expect, loginOffice, test, unique, urls } from "../../src/stack"
import { expectNoRawValues } from "../../src/test"

/** Stage one opening import through the import API and wait until its worker completes it. */
async function stage(
  request: APIRequestContext,
  api: Api,
  profile: string,
  rows: Record<string, string>[]
) {
  const batch = await command<{ id: string; version: number }>(request, api, "POST", "/imports", {
    payload: {
      profile,
      filename: `${profile}.csv`,
      file_hash: "e".repeat(64),
      expected_rows: rows.length,
      mapping: {},
    },
  })
  const chunk = await command<{ version: number }>(
    request,
    api,
    "PUT",
    `/imports/${batch.id}/chunks/0`,
    {
      expected_version: batch.version,
      payload: { rows: rows.map((fields, i) => ({ row_number: i + 1, fields })) },
    }
  )
  const valid = await command<{ version: number; validation_fingerprint: string }>(
    request,
    api,
    "POST",
    `/imports/${batch.id}/validate`,
    { expected_version: chunk.version, payload: { expected_chunks: 1 } }
  )
  await command(request, api, "POST", `/imports/${batch.id}/apply`, {
    expected_version: valid.version,
    payload: { validation_fingerprint: valid.validation_fingerprint },
  })
  await expect
    .poll(async () => (await api.get<{ state: string }>(api.ws(`/imports/${batch.id}`))).state)
    .toBe("completed")
}

/** A sent 10.00 + 21% invoice of the workspace's own customer: its number. */
async function sentInvoice(api: Api) {
  const tag = unique()
  const { relation_id } = await api.post<{ relation_id: string }>(api.ws("/relations"), {
    type: "business",
    company_name: `Opening customer ${tag}`,
    contact: { first_name: "Ada", last_name: `Miller ${tag}`, email: `ada-${tag}@example.test` },
    addresses: [
      { address: "Mill Lane", house_number: "4", zipcode: "1234AB", city: "Utrecht", country: "NL", type: "billing", main_address: true },
    ],
  })
  const { invoice_id } = await api.post<{ invoice_id: string }>(api.ws("/invoices"), {
    relation_id,
    items: [{ title: "Opening fee", quantity: "1", price: "10", vat: "21" }],
  })
  const contact = psql(`SELECT id FROM contacts WHERE relation_id = '${relation_id}'::uuid LIMIT 1`)
  await api.post(api.ws(`/invoices/${invoice_id}/send`), {
    addressed_contact_id: contact,
    address: { address: "Mill Lane", house_number: "4", zipcode: "1234AB", city: "Utrecht", country: "NL" },
    to: [`ada-${tag}@example.test`],
    subject: "Invoice {invoice.invoice_nr}",
    body: "<p>Total {invoice.total}</p>",
    send_method: "manual",
  })
  return psql(`SELECT invoice_number FROM invoices WHERE id = '${invoice_id}'::uuid`)
}

const code = async (api: Api, role: string) => {
  const setup = await api.get<{ account_mappings: Record<string, string> }>(
    api.ws("/accounting/setup")
  )
  const accounts = await api.get<{ id: string; code: string }[]>(
    api.ws("/accounting/accounts?page_size=100")
  )
  return accounts.find((a) => a.id === setup.account_mappings[role])!.code
}

test("review the opening, fix it, activate, then correct an opening balance", async ({ page, request }) => {
  const w = await freshWorkspace(request, "acc-opening")
  await readyBook(request, w.api)
  const number = await sentInvoice(w.api)
  const receivable = { invoice_number: number, face: "12.10", settled: "0", credited: "0", refunded: "0", outstanding: "12.10", carrying: "12.10" }
  await stage(request, w.api, "opening_receivables", [receivable])
  // An unbalanced trial balance first: the review names it in words.
  await stage(request, w.api, "opening_trial_balance", [
    { account_code: await code(w.api, "receivables"), debit: "12.10" },
    { account_code: await code(w.api, "opening_equity"), credit: "10.00" },
  ])

  await loginOffice(page, w.login)
  await page.goto(`${urls.office}/accounting/opening`)
  await expect(page.getByRole("heading", { name: "Opening and activation" })).toBeVisible()
  await expect(page.getByText("Open customer balances")).toBeVisible()
  await page.getByRole("button", { name: "Review the opening" }).click()
  const review = page.getByRole("region", { name: "Review" })
  await expect(review.getByText("1 thing to fix before activation")).toBeVisible()
  await expect(review.getByText("The opening trial balance debits and credits must be equal.")).toBeVisible()
  await expect(page.getByRole("button", { name: "Activate the books" })).toHaveCount(0)

  // Fixed batch: the review picks the newest completed one per kind.
  await stage(request, w.api, "opening_trial_balance", [
    { account_code: await code(w.api, "receivables"), debit: "12.10" },
    { account_code: await code(w.api, "opening_equity"), credit: "12.10" },
  ])
  await page.reload()
  await page.getByRole("button", { name: "Review the opening" }).click()
  await expect(review.getByText("Everything checks out. The books can be activated.")).toBeVisible()
  await expect(review.getByText("12.10").first()).toBeVisible()
  const activate = page.getByRole("button", { name: "Activate the books" })
  await expect(activate).toBeDisabled()
  await page.getByLabel("I reviewed these figures and want to activate the books.").check()
  await activate.click()
  await expect(page.getByText("The books are active.")).toBeVisible()
  const setup = await w.api.get<{ state: string }>(w.api.ws("/accounting/setup"))
  expect(setup.state).toBe("active")

  // Active: the opening balance and a typed correction (review, then book).
  await expect(page.getByText(`Invoice ${number}`)).toBeVisible()
  await expect(page.getByText(/^Open 12\.10 [A-Z]{3}$/)).toBeVisible()
  await page.getByRole("button", { name: "Correct this balance" }).click()
  const form = page.getByRole("form", { name: "Correct an opening balance" })
  await form.getByLabel("Correct open amount").fill("10.00")
  await form.getByLabel("Why").fill("Customer agreed discount before cutover")
  await form.getByLabel("Proof (document or reference)").fill(`MAIL-${unique()}`)
  await form.getByRole("button", { name: "Review the correction" }).click()
  await expect(form.getByText(/goes from 12\.10 to 10\.00/)).toBeVisible()
  await form.getByRole("button", { name: "Book the correction" }).click()
  await expect(page.getByText("Correction booked.")).toBeVisible()
  await expect(page.getByText(/^Open 10\.00 [A-Z]{3}/)).toBeVisible()
  await expect(page.getByText(/corrected once/)).toBeVisible()
  await expectNoRawValues(page)
})
