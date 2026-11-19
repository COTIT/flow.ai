import { readFileSync } from "node:fs"
import { join } from "node:path"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { ExecutionStatus } from "./execution-status"
import { PurchaseApprovalCard } from "./purchase-approval-card"
import type { ApprovalRequestView, ExecutionView } from "./types"

const en = JSON.parse(
  readFileSync(join(__dirname, "../../../../i18n/messages/en.json"), "utf8")
).businessApprovals as Record<string, Record<string, string>>
const t = (key: string, values?: Record<string, string | number>) => {
  const [g, n] = key.split(".") as [string, string]
  let s = en[g]![n]!
  for (const [k, v] of Object.entries(values ?? {}))
    s = s.replace(`{${k}}`, String(v))
  return s
}

const request = (
  over: Partial<ApprovalRequestView> = {}
): ApprovalRequestView => ({
  id: "r2",
  workflow_key: "wf01_purchase",
  pair_id: "p1",
  subject_ref: "odoo:c1:purchase.order:18",
  revision: 1,
  fingerprint: "1a7e",
  status: "pending",
  blocked_reasons: [],
  invalidated_reason: null,
  amount: { minor: 30000, currency_ref: "EUR", decimals: 2 },
  required_permission: "integration.approve",
  requester: { user_id: "u1", name: "Ana Buyer" },
  decision: null,
  can_decide: true,
  execution: null,
  open_incidents: [],
  created_at: "",
  updated_at: "",
  snapshot: {
    v: 1,
    source: {
      connection_id: "c1",
      model: "purchase.order",
      id: "18",
      name: "P00018",
      state: "draft",
    },
    target: { connection_id: "c3", record_type: "purchaseOrder" },
    company: { source_id: "1", target_id: "3", label: "Acme BV" },
    vendor: { source_id: "43", target_id: "9", label: "Deco Addict" },
    currency: { source_id: "1", target_id: "1", ref: "EUR", decimals: 2 },
    location: null,
    requester: null,
    lines: [
      {
        source_line_id: "55",
        product: { source_id: "9", target_id: "1", label: "Desk" },
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
  },
  ...over,
})
const html = (r: ApprovalRequestView) =>
  renderToStaticMarkup(<PurchaseApprovalCard request={r} t={t} locale="en" />)

describe("PurchaseApprovalCard", () => {
  it("shows Odoo's untaxed, tax and total as an estimate", () => {
    const out = html(request())
    expect(out).toContain("€300.00")
    expect(out).toContain("€63.00")
    expect(out).toContain("€363.00")
    expect(out).toContain("Odoo estimate; NetSuite calculates tax")
  })

  it("shows decision buttons only when the API says can_decide", () => {
    expect(html(request())).toContain(">Approve<")
    expect(html(request({ can_decide: false }))).not.toContain(">Approve<")
    expect(html(request({ can_decide: false }))).toContain("You can&#x27;t decide this request.")
  })

  it("lists blocked reasons in words with a mapping link", () => {
    const out = renderToStaticMarkup(
      <PurchaseApprovalCard
        request={request({
          status: "blocked",
          can_decide: false,
          blocked_reasons: [
            { code: "unmapped", entity: "vendor", source_id: "43" },
          ],
        })}
        mappingHref={(b) => `/m?entity=${b.entity}`}
        t={t}
        locale="en"
      />
    )
    expect(out).toContain("Vendor 43 has no NetSuite match")
    expect(out).toContain('href="/m?entity=vendor"')
    expect(out).not.toContain("unmapped")
  })
})

describe("ExecutionStatus", () => {
  const exec = (over: Partial<ExecutionView>): ExecutionView => ({
    id: "e1",
    status: "succeeded",
    phase: "done",
    reason: null,
    resolution: null,
    error_class: null,
    error_message: null,
    target_ref: "netsuite:c3:purchaseOrder:8812",
    verified: true,
    updated_at: "",
    ...over,
  })
  it("says Verified only when the API says verified", () => {
    const on = renderToStaticMarkup(
      <ExecutionStatus execution={exec({})} t={t} />
    )
    expect(on).toContain("Verified in NetSuite")
    const off = renderToStaticMarkup(
      <ExecutionStatus
        execution={exec({
          status: "resolved_manual",
          resolution: "linked_with_differences",
          verified: false,
        })}
        t={t}
      />
    )
    expect(off).not.toContain("Verified in NetSuite")
    expect(off).toContain("Linked to an existing order with differences")
  })
  it("offers only the API's allowed actions", () => {
    const out = renderToStaticMarkup(
      <ExecutionStatus
        execution={exec({
          status: "awaiting_reconciliation",
          verified: false,
          target_ref: null,
          allowed_actions: ["link_existing", "close_no_po"],
        })}
        onAction={() => {}}
        t={t}
      />
    )
    expect(out).toContain("Link existing order")
    expect(out).toContain("Close without order")
    expect(out).not.toContain("Allow one more try")
  })
})
