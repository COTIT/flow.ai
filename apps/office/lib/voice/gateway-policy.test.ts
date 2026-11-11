import { readFileSync } from "node:fs"
import { execFileSync } from "node:child_process"
import { describe, expect, it } from "vitest"

const policies = JSON.parse(
  readFileSync(new URL("./operation-policy.json", import.meta.url), "utf8")
)
const internal = [
  "payments_gateway_refresh",
  "payments_gateway_resolve",
  "payments_globadyme_callback_retire",
  "invoices_gateway_refund",
]
const external = [
  "payments_globadyme_webhook",
  "invoices_gateway_create",
  "invoices_gateway_status",
  "invoices_gateway_qr",
]
const reads = [
  "payments_gateway_list",
  "payments_gateway_get",
  "invoices_gateway_inspect",
]

describe("gateway voice policy", () => {
  it("keeps money, reconciliation, account administration and public flows outside autonomous tools", () => {
    for (const id of internal)
      expect(policies[id].disposition, id).toBe("internal")
    for (const id of external)
      expect(policies[id].disposition, id).toBe("external")
    for (const id of reads) {
      expect(policies[id].disposition, id).toBe("execute")
      expect(policies[id].method, id).toBe("GET")
      expect(policies[id].effects, id).toEqual([])
    }
  })

  it("keeps list and per-owner detail permissions distinct", () => {
    expect(policies.payments_gateway_list.authorization.anyOf).toEqual([
      "invoice.list",
      "order.list",
    ])
    for (const id of ["payments_gateway_get", "payments_gateway_refresh"]) {
      expect(policies[id].authorization.anyOf, id).toEqual([
        "invoice.view",
        "order.view",
      ])
      expect(policies[id].authorization.predicates.join(" "), id).toContain(
        "invoice-owned"
      )
      expect(policies[id].authorization.predicates.join(" "), id).toContain(
        "order-owned"
      )
    }
    expect(policies.invoices_gateway_inspect.authorization.allOf).toEqual([
      "invoice.view",
    ])
    expect(policies.invoices_gateway_refund.authorization.allOf).toEqual([
      "invoice.credit",
    ])
    expect(policies.orders_refund.authorization.predicates.join(" ")).toContain(
      "Globadyme refunds reject assistant execution"
    )
  })

  it("builds explicit gateway contracts without generating the stale catalog", () => {
    const output = execFileSync(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        `
      import {readFileSync} from 'node:fs';
      import {buildCatalog} from './scripts/generate-voice-catalog.mjs';
      const all=JSON.parse(readFileSync('./lib/voice/operation-policy.json','utf8'));
      const ids=${JSON.stringify([...internal, ...external, ...reads])};
      const policy=Object.fromEntries(ids.map(id=>[id,all[id]]));
      const paths={};
      for(const [id,p] of Object.entries(policy)) {
        paths[p.path] ??= {};
        paths[p.path][p.method.toLowerCase()]={operationId:id,tags:['payments'],security:p.authorization.requirements,responses:{200:{description:'OK'}}};
      }
      process.stdout.write(JSON.stringify(buildCatalog({paths,components:{schemas:{}}},policy).filter(op=>op.disposition==='execute').map(op=>op.id).sort()));
    `,
      ],
      { cwd: process.cwd(), encoding: "utf8" }
    )
    expect(JSON.parse(output)).toEqual([...reads].sort())
  })
})
