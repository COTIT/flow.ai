import assert from "node:assert/strict"
import { test } from "vitest"
import { createServer } from "node:http"
import { capacityPlan, createFaultProxy } from "./check-voice-capacity.mjs"

const valid = {
  image: "sha256:" + "a".repeat(64),
  apiUrl: "http://127.0.0.1:3280/v1",
  databaseUrl:
    "postgres://owner:private@127.0.0.1:54320/genesy_assistant_checks_capacity",
  checkpointUrl:
    "postgres://assistant_checkpoint_local:private@127.0.0.1:54320/genesy_assistant_checks_capacity",
  basePort: 3290,
}
test("permits only the two actual deployment memory ceilings", () => {
  assert.deepEqual(capacityPlan(valid).limits, [384, 512])
})
for (const [name, change] of [
  ["remote API", { apiUrl: "https://production.example/v1" }],
  ["URL credentials", { apiUrl: "http://secret@127.0.0.1:3280/v1" }],
  [
    "shared database",
    { databaseUrl: "postgres://owner:private@127.0.0.1:54320/genesy" },
  ],
  [
    "remote database",
    {
      databaseUrl:
        "postgres://owner:private@production.example/genesy_assistant_checks_capacity",
    },
  ],
  [
    "different checkpoint database",
    {
      checkpointUrl:
        "postgres://assistant_checkpoint_local:private@127.0.0.1:54320/genesy_assistant_checks_other",
    },
  ],
  [
    "unrestricted checkpoint login",
    {
      checkpointUrl:
        "postgres://owner:private@127.0.0.1:54320/genesy_assistant_checks_capacity",
    },
  ],
  ["mutable image", { image: "node:latest" }],
  ["invalid loopback port", { basePort: 80 }],
])
  test(`refuses ${name}`, () =>
    assert.throws(() => capacityPlan({ ...valid, ...change })))

test("drops only the armed response after exactly one upstream commit", async () => {
  let writes = 0
  const api = createServer((_request, response) => {
    writes++
    response.writeHead(201, { "Content-Type": "application/json" })
    response.end('{"data":{"order_id":"synthetic"}}')
  })
  await new Promise((resolve) => api.listen(0, "127.0.0.1", resolve))
  const proxy = await createFaultProxy(`http://127.0.0.1:${api.address().port}`)
  try {
    const fault = proxy.arm("test-action")
    let settled = false
    const sent = fetch(
      `http://127.0.0.1:${proxy.port}/v1/workspace/test/orders`,
      { method: "POST", headers: { "Assistant-Action-Id": "test-action" } }
    )
    sent.then(
      () => {
        settled = true
      },
      () => {
        settled = true
      }
    )
    await fault.committed
    await new Promise((resolve) => setTimeout(resolve, 50))
    assert.equal(writes, 1)
    assert.equal(
      settled,
      false,
      "Response must remain unacknowledged after commit"
    )
    fault.release()
    await assert.rejects(sent)
    const ordinary = await fetch(
      `http://127.0.0.1:${proxy.port}/v1/workspace/test/orders`,
      { method: "POST", headers: { "Assistant-Action-Id": "other-action" } }
    )
    assert.equal(ordinary.status, 201)
    assert.equal(writes, 2)
  } finally {
    await proxy.close()
    await new Promise((resolve) => api.close(resolve))
  }
})
