import assert from "node:assert/strict"
import { test } from "node:test"

import { coverage, moduleOf, officeOperations, verbOf } from "./coverage.mjs"

const catalog = [
  { id: "orders_list", domain: "orders", method: "GET", path: "/workspace/{workspace}/orders", disposition: "execute", risk: "read" },
  { id: "orders_cancel", domain: "orders", method: "POST", path: "/workspace/{workspace}/orders/{order}/cancel", disposition: "execute", risk: "high" },
  { id: "till_shift_open", domain: "till", method: "POST", path: "/workspace/{workspace}/till/shift/open", disposition: "external", risk: "write", reason: "Device flow" },
  { id: "webhooks_create", domain: "webhooks", method: "POST", path: "/workspace/{workspace}/webhooks", disposition: "secure_ui", risk: "secret" },
]

test("reads the operations Office imports from the generated client, whatever helper name it uses", () => {
  const sources = [
    `import { useOrdersList, getOrdersListQueryKey } from "@genesy/api-client/endpoints/orders"`,
    `import {\n  useOrdersCancel,\n  type OrdersCancelBody,\n} from "@genesy/api-client/endpoints/orders"`,
    `import { tillShiftOpen } from "@genesy/api-client/endpoints/till"`,
    `import { useSession } from "@genesy/api-client/auth"`,
  ]
  assert.deepEqual([...officeOperations(sources)].sort(), ["orders_cancel", "orders_list", "till_shift_open"])
})

test("names the verb and module of an operation", () => {
  assert.equal(verbOf("orders_list"), "list")
  assert.equal(verbOf("orders_get"), "find")
  assert.equal(verbOf("orders_create"), "create")
  assert.equal(verbOf("orders_update"), "update")
  assert.equal(verbOf("orders_cancel"), "cancel")
  assert.equal(verbOf("orders_events"), "explain")
  assert.equal(verbOf("orders_sales_summary"), "compare")
  assert.equal(verbOf("invoices_preview"), "export")
  assert.equal(verbOf("orders_refund"), "undo")
  assert.equal(moduleOf("farm_payouts"), "supply")
  assert.equal(moduleOf("lots"), "stock")
  assert.equal(moduleOf("invoice_payments"), "invoices")
})

test("lists Office operations voice cannot reach and fills the module × verb grid", () => {
  const report = coverage(catalog, new Set(["orders_list", "orders_cancel", "till_shift_open", "orders_ghost"]))
  assert.deepEqual(
    report.unreachable.map((u) => [u.id, u.why]),
    [
      ["orders_ghost", "not in voice catalog"],
      ["till_shift_open", "external: Device flow"],
    ]
  )
  assert.deepEqual(report.grid.orders.list, ["orders_list"])
  assert.deepEqual(report.grid.orders.cancel, ["orders_cancel"])
  assert.deepEqual(report.grid.till.update ?? [], [])
  assert.deepEqual(report.grid.admin.create, ["webhooks_create"])
  assert.match(report.markdown, /\| orders \| 0 \| 1 \| 0 \| 0 \| 1 \|/)
  assert.match(report.markdown, /till_shift_open/)
})
