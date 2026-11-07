import assert from "node:assert/strict"
import { test } from "vitest"
import { nativePlan, nativeReviewReply, verifyNativeCard } from "./native.mjs"

const workspaceId = "01900000-0000-7000-8000-000000000001"
const valid = {
  formatVersion: 1,
  caseId: "product-create",
  sourceTest: "products::create_writes_default_variant_and_price_history",
  apiOrigin: "http://127.0.0.1:43127",
  databaseName: "_sqlx_test_" + "a".repeat(52),
  workspaceId,
  bearer: "b".repeat(64),
  testBridgeSecret: "genesy-native-reviewed-test-bridge-key-2026",
  request: { method: "POST", path: `/v1/workspace/${workspaceId}/products`, body: { name: "A", price: "220", gross_price: "200" } },
}
const image = "sha256:" + "c".repeat(64)
test("allows only a source-reviewed synthetic native fixture", () => {
  assert.equal(nativePlan(valid, image).image, image)
})
for (const [name, change] of [
  ["shared database", { databaseName: "genesy" }],
  ["remote origin", { apiOrigin: "https://production.example" }],
  ["origin credentials", { apiOrigin: "http://secret@127.0.0.1:43127" }],
  ["unknown case", { caseId: "arbitrary-write" }],
  ["wrong source test", { sourceTest: "products::other" }],
  ["other workspace path", { request: { ...valid.request, path: "/v1/workspace/01900000-0000-7000-8000-000000000002/products" } }],
  ["changed amount", { request: { ...valid.request, body: { ...valid.request.body, price: "221" } } }],
  ["secret-bearing extra field", { request: { ...valid.request, body: { ...valid.request.body, password: "never-send" } } }],
]) test(`refuses ${name}`, () => assert.throws(() => nativePlan({ ...valid, ...change }, image)))
test("refuses a mutable image before running paid traffic", () => assert.throws(() => nativePlan(valid, "node:latest")))

test("model reply excludes private review proof and untrusted response fields", () => {
  const reply = nativeReviewReply({ status: "requires_confirmation", data: { actionId: "owned-action", confirmationCode: "654321" }, review: { ticket: "private-ticket", confirmationPhrase: "private-phrase" }, message: "private-message" })
  assert.equal(reply.actionId, "owned-action")
  for (const secret of ["654321", "private-ticket", "private-phrase", "private-message"]) assert(!JSON.stringify(reply).includes(secret))
})

test("independent card approval refuses changed scope and expired proof", () => {
  const plan = nativePlan(valid, image)
  const value = { status: "requires_confirmation", review: { details: { workspace: workspaceId, changes: valid.request.body, target: {} }, ticket: "test-only-proof-" + "x".repeat(24), expiresAt: new Date(Date.now() + 60000).toISOString() } }
  verifyNativeCard(value, plan)
  for (const change of [
    { details: { ...value.review.details, workspace: "another-workspace" } },
    { details: { ...value.review.details, changes: { ...valid.request.body, price: "221" } } },
    { details: { ...value.review.details, target: { product: "another-product" } } },
    { expiresAt: "2020-01-01T00:00:00Z" },
    { batchId: "unexpected-batch" },
  ]) assert.throws(() => verifyNativeCard({ ...value, review: { ...value.review, ...change } }, plan))
})
