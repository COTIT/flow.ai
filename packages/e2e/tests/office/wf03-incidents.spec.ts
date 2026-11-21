import { expectNoRawValues } from "../../src/test"
import { meta1 } from "../business-approvals/fixtures"
import {
  emptyOverview,
  expect,
  expectFits,
  openMenu,
  test,
  type FakeApi,
} from "./fake-api"

// WF-03 sync problems against the mocked API. Shapes from contracts.md §11.7 (Sample data).

const late = {
  id: "i1",
  kind: "exchange_late",
  subject_ref: "odoo:c1:purchase.order:17",
  status: "open",
  pair_id: "p1",
  approval_request_id: "r5",
  execution_id: "e2",
  detail: {},
  note: null,
  resolved_by: null,
  created_at: "2026-11-21T09:00:00Z",
  category: "unknown_outcome",
  diagnosis: {
    sentence:
      "We sent the record but could not confirm it arrived. Check the other system before trying again.",
    records: [{ subject_ref: "odoo:c1:purchase.order:17", target_id: null }],
    last_attempt: { attempt_no: 1, outcome: "ambiguous", http_status: null },
  },
  verified_at: null,
  resolution_kind: null,
}
const refused = {
  ...late,
  id: "i2",
  kind: "mapping_invalid",
  category: "data",
  approval_request_id: null,
  execution_id: null,
  diagnosis: {
    ...late.diagnosis,
    sentence:
      "The other system refused a mapped value. Check the mappings for this pair.",
  },
}
const detailOf = (i: typeof late) => ({
  ...i,
  expectation: i.execution_id
    ? {
        status: "late",
        due_at: "2026-11-21T09:15:00Z",
        checks: 3,
        last_checked_at: "2026-11-21T09:30:00Z",
        target_kind: "purchaseOrder",
        target_external_id: "vz-0f1e2d3c4b5a69788796a5b4c3d2e1f0",
      }
    : null,
  timeline: [
    {
      kind: "write_intent",
      actor_kind: "system",
      created_at: "2026-11-21T09:00:00Z",
      summary: "Ready to create the order",
    },
    {
      kind: "expectation_late",
      actor_kind: "system",
      created_at: "2026-11-21T09:15:00Z",
      summary: "Order not seen in time",
    },
  ],
})
const list = <T>(rows: T[]) => ({
  data: rows,
  meta: {
    ...meta1,
    from: rows.length ? 1 : null,
    to: rows.length || null,
    total: rows.length,
  },
})

/** Stateful incidents: verify finds the record (i1 → reconciled), resolve closes by hand. */
function incidentsApi(api: FakeApi, rows = [late, refused]) {
  const byId = new Map(
    rows.map((r) => [r.id, structuredClone(r) as typeof late])
  )
  return api
    .on("GET", "/workspace/:ws/workflows/incidents", ({ query }) =>
      list(
        [...byId.values()].filter(
          (r) =>
            r.status === (query.get("status") ?? "open") &&
            (!query.get("category") || r.category === query.get("category"))
        )
      )
    )
    .on("GET", "/workspace/:ws/workflows/incidents/:id", ({ params }) => ({
      data: detailOf(byId.get(params[1]!)!),
    }))
    .on(
      "POST",
      "/workspace/:ws/workflows/incidents/:id/verify",
      ({ params }) => {
        const r = byId.get(params[1]!)!
        Object.assign(r, {
          status: "resolved",
          verified_at: "2026-11-21T10:00:00Z",
          resolution_kind: "reconciled",
        })
        return {
          data: {
            ...detailOf(r),
            expectation: { ...detailOf(r).expectation, status: "met" },
          },
        }
      }
    )
    .on(
      "POST",
      "/workspace/:ws/workflows/incidents/:id/resolve",
      ({ params, body }) => {
        const r = byId.get(params[1]!)!
        Object.assign(r, {
          status: "resolved",
          note: (body as { note: string }).note,
          resolution_kind: "manual_closed",
          resolved_by: "u2",
        })
        return { data: r }
      }
    )
}

test("lists sync problems by category, shows the sentence and timeline, Check now closes it", async ({
  page,
  api,
}) => {
  incidentsApi(api)
  emptyOverview(api)
  await page.goto("/")
  await openMenu(page)
  await page.getByRole("link", { name: "Sync problems" }).click()
  await expect(
    page.getByRole("heading", { name: "Sync problems", level: 1 })
  ).toBeVisible()
  const items = page.getByTestId("incident-list").getByRole("button")
  await expect(items).toHaveCount(2)
  await page.getByRole("button", { name: "Data", exact: true }).click()
  await expect(items).toHaveCount(1)
  await expect(items.first()).toContainText("Mapping refused")
  await page.getByRole("button", { name: "All", exact: true }).click()
  await expect(items).toHaveCount(2)
  await expectFits(page)

  await items.filter({ hasText: "Not seen in time" }).click()
  const detail = page.getByTestId("incident-detail")
  await expect(
    detail.getByRole("heading", { name: "Not seen in time" })
  ).toBeVisible()
  await expect(detail.getByText("Outcome unknown")).toBeVisible()
  await expect(detail.getByText(/could not confirm it arrived/)).toBeVisible()
  await expect(detail.getByText("3 of 10 checks used")).toBeVisible()
  await expect(detail.getByText("Order not seen in time")).toBeVisible()
  await expectFits(page)
  await expectNoRawValues(page)

  await detail.getByRole("button", { name: "Check now" }).click()
  await expect(
    page.getByText("Checked. The record matches: problem closed.")
  ).toBeVisible()
  await expect(
    detail.getByText("Closed after a check confirmed the record")
  ).toBeVisible()
  const call = api.callsTo(
    "POST",
    "/workspace/:ws/workflows/incidents/:id/verify"
  )
  expect(call).toHaveLength(1)
  expect(call[0]!.headers["idempotency-key"]).toMatch(/^[0-9a-f-]{36}$/)
  await expectNoRawValues(page)
})

test("closes a refused record by hand with a note", async ({ page, api }) => {
  incidentsApi(api)
  await page.goto("/integrations/incidents")
  await page
    .getByTestId("incident-list")
    .getByRole("button", { name: /Mapping refused/ })
    .click()
  const detail = page.getByTestId("incident-detail")
  await expect(detail.getByRole("button", { name: "Check now" })).toHaveCount(0)
  await detail.getByRole("button", { name: "Close by hand" }).click()
  await detail.getByLabel("What did you do?").fill("Fixed the vendor mapping")
  await expectFits(page)
  await detail.getByRole("button", { name: "Close problem" }).click()
  await expect(detail.getByText("Closed by hand").first()).toBeVisible()
  const call = api.callsTo(
    "POST",
    "/workspace/:ws/workflows/incidents/:id/resolve"
  )
  expect(call[0]!.body).toEqual({
    note: "Fixed the vendor mapping",
    resolution_kind: "manual_closed",
  })
  await expectNoRawValues(page)
})

test("no problems shows the calm empty state", async ({ page, api }) => {
  incidentsApi(api, [])
  await page.goto("/integrations/incidents")
  await expect(
    page.getByText("No sync problems. Everything Flow sent has arrived.")
  ).toBeVisible()
  await expectFits(page)
})

test.describe("load error", () => {
  test.use({
    allow: {
      responses: [
        {
          url: "/v1/workspace/w1/workflows/incidents?status=open",
          status: 500,
        },
      ],
    },
  })

  test("a failed load says so in words with Retry", async ({ page, api }) => {
    api.on("GET", "/workspace/:ws/workflows/incidents", () => ({
      status: 500,
      body: { message: "Server Error" },
    }))
    await page.goto("/integrations/incidents")
    await expect(
      page.getByText("Sync problems could not be loaded.")
    ).toBeVisible({ timeout: 20_000 }) // the query retries before it gives up
    await expect(page.getByRole("button", { name: "Try again" })).toBeVisible()
    await expectNoRawValues(page)
  })
})
