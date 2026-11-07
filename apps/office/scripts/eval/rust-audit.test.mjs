import assert from "node:assert/strict"
import { test } from "vitest"
import { auditHTTP, parseTestResults, matchReviews } from "./rust-audit.mjs"

const catalog = [
  {
    id: "products_get",
    domain: "products",
    method: "GET",
    disposition: "execute",
  },
  {
    id: "products_create",
    domain: "products",
    method: "POST",
    disposition: "execute",
  },
]
const observation = {
  formatVersion: 1,
  operationId: "products_get",
  method: "GET",
  status: 200,
  test: "products::read",
  contractValidated: true,
}

test("only a passed full test upgrades an executed request to candidate evidence", () => {
  const results = parseTestResults(
    "test products::read ... ok\ntest products::bad ... FAILED\ntest products::skipped ... ignored\n"
  )
  assert.equal(results.size, 3)
  const result = auditHTTP(
    catalog,
    [
      observation,
      { ...observation, test: "products::bad" },
      { ...observation, test: "unnamed" },
    ],
    results
  )
  assert.equal(result.operations[0].passedTestObservations.length, 1)
  assert.equal(result.operations[0].unprovenObservations.length, 2)
  assert.equal(result.operations[0].assertionReview, "missing")
  assert.equal(result.operations[1].status, "missing")
  assert.equal(result.formalResultPasses, 0)
})

test("a later failed rerun cannot be erased by a successful test line", () => {
  const results = parseTestResults(
    "test products::read ... FAILED\ntest products::read ... ok\n"
  )
  assert.equal(results.get("products::read"), "FAILED")
})

test("unknown operations, wrong methods and unvalidated responses fail closed", () => {
  for (const extra of [
    { operationId: "not_in_catalog" },
    { method: "POST" },
    { contractValidated: false },
    { status: 999 },
  ])
    assert.throws(
      () => auditHTTP(catalog, [{ ...observation, ...extra }], new Map()),
      /observation/
    )
})

test("reviewed business gates require matching operation/status, passed test and pinned assertion sources", () => {
  const review = {
    id: "product",
    operations: ["products_get"],
    test: "products::read",
    gates: { result: [200] },
    assertions: [{ file: "test.rs" }],
  }
  const results = new Map([["products::read", "ok"]])
  const match = (records, sources = new Set(["test.rs"])) =>
    matchReviews(catalog, records, results, [review], sources)
  assert.equal(match([observation]).length, 1)
  assert.equal(match([{ ...observation, status: 404 }]).length, 0)
  assert.equal(
    match([{ ...observation, operationId: "products_create", method: "POST" }])
      .length,
    0
  )
  assert.equal(match([observation], new Set()).length, 0)
  results.set("products::read", "FAILED")
  assert.equal(match([observation]).length, 0)
})

test("unavailable native placeholder transport cannot count as a business result", () => {
  const records = matchReviews(
    catalog.map((operation) => ({ ...operation, availability: "unavailable" })),
    [observation],
    new Map([["products::read", "ok"]]),
    [
      {
        id: "placeholder",
        operations: ["products_get"],
        test: "products::read",
        gates: { transport: [200], result: [200] },
        assertions: [{ file: "test.rs" }],
      },
    ],
    new Set(["test.rs"])
  )
  assert.deepEqual(
    records.map((record) => record.gate),
    ["transport"]
  )
})
