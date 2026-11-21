import { randomBytes } from "node:crypto"
import { execFileSync } from "node:child_process"
import type { APIRequestContext } from "@playwright/test"

import {
  apiAs,
  container,
  cred,
  expect,
  loginOffice,
  test,
  unique,
  urls,
  type Api,
} from "../src/stack"
import { expectNoRawValues } from "../src/test"

// WF-03 sync problems against the real API (INTEGRATIONS_PROVIDERS=fake). Each test inserts its
// own approved request, run and incident with SQL (the stack's "test's own rows" rule);
// connections and the pair go through the real API.

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

/** An approved request raised by another member, its run and one incident of `kind`. */
async function seedIncident(
  request: APIRequestContext,
  kind: "mapping_invalid" | "exchange_late"
) {
  const api = await apiAs(request, owner())
  const other = await apiAs(request, cred("office (RUAHA)", "admin"))
  const who = await other.get<{ user: { id: string } }>("/me")
  const me = await api.get<{ user: { id: string } }>("/me")
  const { pairId, odoo } = await pair(api)
  const n = unique()
  const subject = `odoo:${odoo}:purchase.order:${n}`
  const eid = `vz-${randomBytes(16).toString("hex")}`
  const req = sql(`INSERT INTO approval_requests
    (workspace_id, workflow_key, pair_id, subject_ref, revision, fingerprint, snapshot,
     amount_minor, currency_ref, currency_decimals, required_permission, requester_user_id,
     status, decided_by, decided_at)
    VALUES ('${api.workspace}', 'wf01_purchase', '${pairId}', '${subject}', 1, '${hex64()}',
     '{}'::jsonb, 36300, 'EUR', 2, 'integration.approve', '${who.user.id}', 'approved',
     '${me.user.id}', now())
    RETURNING id`)
  const exec =
    sql(`INSERT INTO workflow_executions (workspace_id, approval_request_id,
     workflow_key, correlation_id, status, reason)
    VALUES ('${api.workspace}', '${req}', 'wf01_purchase', '${eid}', 'blocked', 'validation_rejected')
    RETURNING id`)
  const category = kind === "mapping_invalid" ? "data" : "unknown_outcome"
  const sentence =
    kind === "mapping_invalid"
      ? "The other system refused a mapped value. Check the mappings for this pair."
      : "We sent the record but could not confirm it arrived. Check the other system before trying again."
  const diagnosis = JSON.stringify({
    code: "validation",
    sentence,
    records: [],
    last_attempt: null,
  })
  const inc =
    sql(`INSERT INTO workflow_incidents (workspace_id, kind, subject_ref, pair_id,
     approval_request_id, execution_id, status, detail, category, diagnosis)
    VALUES ('${api.workspace}', '${kind}', '${subject}', '${pairId}', '${req}', '${exec}', 'open',
     '{}', '${category}', $$${diagnosis}$$::jsonb)
    RETURNING id`)
  sql(`INSERT INTO exchange_expectations (workspace_id, workflow_key, correlation_id,
     execution_id, source_ref, target_kind, target_external_id, due_at, status, incident_id)
    VALUES ('${api.workspace}', 'wf01_purchase', '${eid}', '${exec}', '${subject}',
     'purchaseOrder', '${eid}', now() + interval '1 hour',
     '${kind === "mapping_invalid" ? "failed" : "late"}', '${inc}')`)
  return { inc, sentence }
}

test("closes a refused record by hand: incident manual_closed, expectation closed", async ({
  page,
  request,
}) => {
  const r = await seedIncident(request, "mapping_invalid")
  await loginOffice(page, owner())
  await page.goto(`${urls.office}/integrations/incidents`)
  await page.getByRole("button", { name: "Data", exact: true }).click()
  await page
    .getByTestId("incident-list")
    .getByRole("button", { name: /Mapping refused/ })
    .first()
    .click()
  const detail = page.getByTestId("incident-detail")
  await expect(detail.getByText(r.sentence)).toBeVisible()
  await detail.getByRole("button", { name: "Close by hand" }).click()
  await detail.getByLabel("What did you do?").fill("Fixed the vendor mapping")
  await detail.getByRole("button", { name: "Close problem" }).click()
  await expect(detail.getByText("Closed by hand").first()).toBeVisible()
  await expectNoRawValues(page)
  expect(
    sql(
      `SELECT status || ':' || resolution_kind FROM workflow_incidents WHERE id = '${r.inc}'`
    )
  ).toBe("resolved:manual_closed")
  expect(
    sql(
      `SELECT status FROM exchange_expectations WHERE incident_id = '${r.inc}'`
    )
  ).toBe("closed")
})

test("Check now on a late record reads back once and answers in words", async ({
  page,
  request,
}) => {
  const r = await seedIncident(request, "exchange_late")
  await loginOffice(page, owner())
  await page.goto(`${urls.office}/integrations/incidents`)
  await page
    .getByTestId("incident-list")
    .getByRole("button", { name: /Not seen in time/ })
    .first()
    .click()
  const detail = page.getByTestId("incident-detail")
  await expect(detail.getByText(r.sentence)).toBeVisible()
  await detail.getByRole("button", { name: "Check now" }).click()
  // The fake provider has no orders: the record is still not there; nothing is closed.
  await expect(
    page.getByText("Checked. The record is still not there.")
  ).toBeVisible()
  await expectNoRawValues(page)
  expect(
    sql(`SELECT status FROM workflow_incidents WHERE id = '${r.inc}'`)
  ).toBe("open")
  expect(
    sql(
      `SELECT checks FROM exchange_expectations WHERE incident_id = '${r.inc}'`
    )
  ).toBe("0")
})
