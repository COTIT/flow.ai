import { randomBytes } from "node:crypto"
import { execFileSync } from "node:child_process"
import type { APIRequestContext } from "@playwright/test"

import {
  apiAs,
  container,
  cred,
  expect,
  loginFlow,
  loginOffice,
  test,
  unique,
  urls,
  type Api,
} from "../src/stack"
import { expectNoRawValues, markInjected } from "../src/test"

// WF-01 business approvals against the real API (INTEGRATIONS_PROVIDERS=fake). C4a's poller is
// not built yet, so each test inserts its own approval request with one SQL statement (the
// stack's "test's own rows" rule); connections and the pair go through the real API.

const owner = () => cred("office (AFM)", "owner")

/** One statement in this stack's Postgres; first line of output (e.g. a RETURNING id). */
function sql(statement: string) {
  return execFileSync(
    "docker",
    [
      "exec",
      container,
      "psql",
      "-U",
      "postgres",
      "-d",
      "genesy",
      "-qAtc",
      statement,
    ],
    { encoding: "utf8" }
  )
    .trim()
    .split("\n")[0]!
}
const hex64 = () => randomBytes(32).toString("hex")

/** An Odoo and a NetSuite connection of this test's own, and the WF-01 pair between them. */
async function pair(api: Api) {
  const tag = unique()
  const odoo = await api.post<{ connection_id: string }>(
    api.ws("/integrations/connections"),
    {
      provider: "odoo",
      label: `Odoo ${tag}`,
      config: { base_url: "https://odoo.e2e.test", database: null },
      secrets: { api_key: `key-${tag}` },
    }
  )
  const netsuite = await api.post<{ connection_id: string }>(
    api.ws("/integrations/connections"),
    {
      provider: "netsuite",
      label: `NetSuite ${tag}`,
      config: {
        rest_domain: "1234567.suitetalk.api.netsuite.com",
        environment: "sandbox",
        client_id: "e2e",
        certificate_id: "kid-e2e",
        alg: "ES256",
      },
      secrets: {
        private_key:
          "-----BEGIN PRIVATE KEY-----\ne2e\n-----END PRIVATE KEY-----",
      },
    }
  )
  const p = await api.post<{ pair_id: string }>(api.ws("/integrations/pairs"), {
    workflow_key: "wf01_purchase",
    source_connection_id: odoo.connection_id,
    target_connection_id: netsuite.connection_id,
  })
  return {
    pairId: p.pair_id,
    odoo: odoo.connection_id,
    netsuite: netsuite.connection_id,
  }
}

/**
 * Inserts one approval request (snapshot per contracts §5.4) and returns its id and order name.
 * `requester` is a user id; `blocked` makes it a blocked request with an unmapped vendor.
 */
async function seedRequest(
  request: APIRequestContext,
  {
    requester,
    blocked = false,
  }: { requester: "other" | "owner"; blocked?: boolean }
) {
  const api = await apiAs(request, owner())
  const other = await apiAs(request, cred("office (RUAHA)", "admin"))
  const who = await (requester === "owner" ? api : other).get<{
    user: { id: string }
  }>("/me")
  const { pairId, odoo, netsuite } = await pair(api)
  const n = unique()
  const name = `P${n}`
  const snapshot = {
    v: 1,
    source: {
      connection_id: odoo,
      model: "purchase.order",
      id: n,
      name,
      state: "draft",
    },
    target: { connection_id: netsuite, record_type: "purchaseOrder" },
    company: { source_id: "1", target_id: "3", label: "Acme BV" },
    vendor: {
      source_id: "43",
      target_id: blocked ? null : "1187",
      label: "Deco Addict",
    },
    currency: { source_id: "1", target_id: "1", ref: "EUR", decimals: 2 },
    location: null,
    requester: { source_id: "2", user_id: who.user.id },
    lines: [
      {
        source_line_id: "55",
        product: { source_id: "9", target_id: "501", label: "Desk" },
        uom: { source_id: "1", target_id: "1", label: "Units" },
        quantity: "2.00000000",
        price_unit: "150.00000000",
        subtotal_minor: 30000,
        date_planned: null,
      },
    ],
    amount_untaxed_minor: 30000,
    amount_tax_minor: 6300,
    amount_total_minor: 36300,
    tax_note: "odoo_estimate_target_calculates",
    mapping_ids: {},
  }
  const id = sql(`INSERT INTO approval_requests
    (workspace_id, workflow_key, pair_id, subject_ref, revision, fingerprint, snapshot,
     amount_minor, currency_ref, currency_decimals, required_permission, requester_user_id,
     status, blocked_reasons)
    VALUES ('${api.workspace}', 'wf01_purchase', '${pairId}', 'odoo:${odoo}:purchase.order:${n}', 1,
     '${hex64()}', $$${JSON.stringify(snapshot)}$$::jsonb, 36300, 'EUR', 2,
     ${blocked ? "NULL" : "'integration.approve'"}, '${who.user.id}',
     '${blocked ? "blocked" : "pending"}', ${blocked ? "ARRAY['unmapped:vendor:43']" : "'{}'"})
    RETURNING id`)
  expect(id).toMatch(/^[0-9a-f-]{36}$/)
  return { id, name, pairId, workspace: api.workspace, api }
}

const decideUrl = (origin: string, ws: string, id: string) =>
  `${origin}/v1/workspace/${ws}/approvals/requests/${id}/approve`

test("approve end to end: decision recorded, NetSuite order queued apart from the approval", async ({
  page,
  request,
}) => {
  const r = await seedRequest(request, { requester: "other" })
  await loginOffice(page, owner())
  await page.goto(`${urls.office}/approvals/purchases`)
  await page
    .getByRole("button", { name: new RegExp(`Odoo order ${r.name.slice(1)}`) })
    .click()

  const card = page.getByTestId("purchase-approval-card")
  await expect(
    card.getByRole("heading", { name: `Odoo order ${r.name}` })
  ).toBeVisible()
  await expect(
    card.getByText("Odoo estimate; NetSuite calculates tax")
  ).toBeVisible()
  await card.getByRole("button", { name: "Approve" }).click()

  await expect(
    page.getByText("Approved. The NetSuite order is queued.")
  ).toBeVisible()
  await expect(card.getByText("Approved", { exact: true })).toBeVisible()
  // The worker may have run `wf01_execute` already (fake providers stop it): any of these, in words.
  await expect(
    card
      .getByTestId("execution-status")
      .getByText(/^(Queued|Creating order|Stopped)$/)
  ).toBeVisible()
  await expectNoRawValues(page)
  // The API recorded one decision and one execution. The stack's worker may already have run
  // `wf01_execute` against the fake providers, which create no NetSuite order: never succeeded.
  expect(sql(`SELECT status FROM approval_requests WHERE id = '${r.id}'`)).toBe(
    "approved"
  )
  expect(
    sql(`SELECT count(*) FROM approval_decisions WHERE request_id = '${r.id}'`)
  ).toBe("1")
  expect(["queued", "running", "blocked"]).toContain(
    sql(
      `SELECT status FROM workflow_executions WHERE approval_request_id = '${r.id}'`
    )
  )
})

test("a request that changed after opening answers 409, said in words; Reload approves the new one", async ({
  page,
  request,
}) => {
  const r = await seedRequest(request, { requester: "other" })
  // The guard lets exactly this request's answer through: its 409 is the point of the test.
  const allow = decideUrl(urls.office, r.workspace, r.id)
  markInjected(page, { test: (u: string) => u === allow })
  let seen409 = false
  page.on("response", (res) => {
    if (res.url() === allow && res.status() === 409) seen409 = true
  })
  await loginOffice(page, owner())
  await page.goto(`${urls.office}/approvals/purchases`)
  await page
    .getByRole("button", { name: new RegExp(`Odoo order ${r.name.slice(1)}`) })
    .click()
  await expect(page.getByRole("button", { name: "Approve" })).toBeVisible()

  // Odoo changed behind the open page: a new fingerprint.
  sql(
    `UPDATE approval_requests SET fingerprint = '${hex64()}', updated_at = now() WHERE id = '${r.id}'`
  )
  await page.getByRole("button", { name: "Approve" }).click()
  await expect(
    page.getByText(
      "This request changed after you opened it. Reload to see the new version."
    )
  ).toBeVisible()
  await expectNoRawValues(page)
  expect(seen409).toBe(true)

  await page.getByRole("button", { name: "Reload" }).click()
  await expect(page.getByRole("button", { name: "Reload" })).toHaveCount(0)
  await page.getByRole("button", { name: "Approve" }).click()
  await expect(
    page.getByText("Approved. The NetSuite order is queued.")
  ).toBeVisible()
})

test("four-eyes: the requester sees words, not buttons, and the API refuses with 403", async ({
  page,
  request,
}) => {
  const r = await seedRequest(request, { requester: "owner" })
  const detail = await r.api.get<{ can_decide: boolean; fingerprint: string }>(
    r.api.ws(`/approvals/requests/${r.id}`)
  )
  expect(detail.can_decide).toBe(false)
  const res = await request.post(decideUrl(urls.api, r.workspace, r.id), {
    headers: { Authorization: `Bearer ${r.api.token}` },
    data: { fingerprint: detail.fingerprint },
  })
  expect(res.status()).toBe(403)
  expect((await res.json()).code).toBe("four_eyes_required")

  await loginOffice(page, owner())
  await page.goto(`${urls.office}/approvals/purchases`)
  await page.getByRole("button", { name: "All", exact: true }).click()
  await page
    .getByRole("button", { name: new RegExp(`Odoo order ${r.name.slice(1)}`) })
    .click()
  const card = page.getByTestId("purchase-approval-card")
  await expect(card.getByText("You can't decide this request.")).toBeVisible()
  await expect(card.getByRole("button", { name: "Approve" })).toHaveCount(0)
  await expect(page.getByText("four_eyes_required")).toHaveCount(0)
  await expectNoRawValues(page)
})

test("a blocked request names the missing vendor match and opens its mappings", async ({
  page,
  request,
}) => {
  const r = await seedRequest(request, { requester: "other", blocked: true })
  await loginOffice(page, owner())
  await page.goto(`${urls.office}/approvals/purchases`)
  await page.getByRole("button", { name: "All", exact: true }).click()
  await page
    .getByRole("button", { name: new RegExp(`Odoo order ${r.name.slice(1)}`) })
    .click()

  const card = page.getByTestId("purchase-approval-card")
  await expect(
    card.getByText("This request cannot be decided yet")
  ).toBeVisible()
  await expect(card.getByText("Vendor 43 has no NetSuite match")).toBeVisible()
  await expect(card.getByRole("button", { name: "Approve" })).toHaveCount(0)
  await expectNoRawValues(page)
  await card.getByRole("link", { name: "Fix in mappings" }).click()
  await expect(page).toHaveURL(
    `${urls.office}/settings/integrations/${r.pairId}/mappings?entity=vendor`
  )
  await expect(
    page.getByRole("heading", { name: "Mappings", level: 1 })
  ).toBeVisible()
  await expect(page.getByText("No mappings yet")).toBeVisible()
})

test("a new connection's secret is sent once and only ever shown masked", async ({
  page,
  request,
}) => {
  const secret = `sk-${unique()}-${hex64().slice(0, 12)}`
  const label = `Odoo ${unique()}`
  const responses: string[] = []
  page.on("response", async (res) => {
    if (
      res.url().includes("/integrations/") &&
      res.request().method() === "GET"
    )
      responses.push(await res.text().catch(() => ""))
  })
  await loginOffice(page, owner())
  await page.goto(`${urls.office}/settings/integrations`)
  await page.getByRole("button", { name: "Add connection" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Name").fill(label)
  await dialog.getByLabel("Odoo address").fill("https://odoo.e2e.test")
  await dialog.getByLabel("API key").fill(secret)
  await dialog.getByRole("button", { name: "Save" }).click()

  await expect(page.getByText("Connection added.")).toBeVisible()
  const row = page.getByRole("listitem").filter({ hasText: label })
  await expect(row.getByLabel("Stored. Leave as is to keep it.")).toHaveText(
    "********"
  )
  expect(await page.content()).not.toContain(secret)
  expect(responses.join("\n")).not.toContain(secret)

  const api = await apiAs(request, owner())
  const list = await api.get<
    { label: string; secrets: Record<string, string | null> }[]
  >(api.ws("/integrations/connections?page_size=100"))
  expect(list.find((c) => c.label === label)?.secrets).toEqual({
    api_key: "********",
  })
  await expectNoRawValues(page, { allow: ["********"] })
})

test("Flow approves with its one primary action", async ({ page, request }) => {
  const r = await seedRequest(request, { requester: "other" })
  await loginFlow(page, owner())
  await page.goto(`${urls.flow}/approvals/purchases`)
  await page
    .getByRole("button", { name: new RegExp(`Odoo order ${r.name.slice(1)}`) })
    .click()
  await page
    .getByTestId("purchase-approval-card")
    .getByRole("button", { name: "Approve" })
    .click()
  await expect(
    page.getByText("Approved. The NetSuite order is queued.")
  ).toBeVisible()
  expect(sql(`SELECT status FROM approval_requests WHERE id = '${r.id}'`)).toBe(
    "approved"
  )
})
