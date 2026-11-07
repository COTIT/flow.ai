import assert from "node:assert/strict"
import { test } from "vitest"
import { readFile } from "node:fs/promises"
import { prepareRead } from "./local.mjs"

const operation = {
  id: "products_get",
  method: "GET",
  path: "/workspace/{workspace}/products/{product}",
  disposition: "execute",
  effects: [],
}
const auth = {
  token: "never-expose",
  workspaceId: "workspace",
  productId: "product",
}
const fixture = {
  request: { path: { product: "{{productId}}" }, query: { q: "two words" } },
  assertions: [{ path: "data.id", equals: "{{productId}}" }],
}

test("local pilot binds reviewed synthetic identifiers without reading bearer into fixtures", () => {
  const result = prepareRead(operation, fixture, auth)
  assert.equal(result.path, "/workspace/workspace/products/product?q=two+words")
  assert.equal(result.assertions[0].equals, "product")
  assert(!JSON.stringify(result).includes(auth.token))
})

test("read pilot refuses writes, secure flows, effectful GET and credential interpolation", () => {
  for (const extra of [
    { method: "POST" },
    { disposition: "secure_ui" },
    { effects: ["email"] },
  ]) {
    assert.throws(
      () => prepareRead({ ...operation, ...extra }, fixture, auth),
      /read-only/
    )
  }
  assert.throws(
    () =>
      prepareRead(
        operation,
        { ...fixture, request: { path: { product: "{{token}}" } } },
        auth
      ),
    /binding/
  )
  assert.throws(
    () =>
      prepareRead(
        operation,
        { ...fixture, request: { path: { product: "../other" } } },
        auth
      ),
    /identifier/
  )
})

test("malformed product identifier fixture matches the native bad-ID404 contract", async () => {
  const dataset = JSON.parse(
    await readFile(new URL("fixtures.json", import.meta.url), "utf8")
  )
  assert.equal(
    dataset.cases.find((item) => item.id === "product-get-malformed-id")
      .expectedStatus,
    404
  )
})
