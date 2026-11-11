import { expect, test } from "@playwright/test"

// The built app's routing: requests straight to Flow's server, no page.

test("addresses outside the allowlist get a JSON 404", async ({ request }) => {
  for (const path of ["/api/voice/secure", "/api/voice/unknown"]) {
    const response = await request.post(path, { data: {} })
    expect(response.status(), path).toBe(404)
    expect(await response.json()).toEqual({ message: expect.any(String) })
  }
})

test("a method the route does not take gets a JSON 405 with Allow", async ({
  request,
}) => {
  const response = await request.get("/api/voice/session")
  expect(response.status()).toBe(405)
  expect(response.headers()["allow"]).toBe("POST, DELETE")
  expect(await response.json()).toEqual({ message: expect.any(String) })
})

test("the bearer reaches the voice server; cookies and forwarding headers do not", async ({
  request,
}) => {
  const response = await request.delete("/api/voice/session", {
    headers: {
      Authorization: "Bearer e2e-token",
      Cookie: "genesy_voice_gap=x",
      "X-Forwarded-For": "1.2.3.4",
    },
    data: { workspaceId: "w1", sessionId: "s1" },
  })
  expect(response.status()).toBe(200)
  expect(await response.json()).toEqual({
    status: "ended",
    authorization: "Bearer e2e-token",
    cookie: null,
    forwardedFor: null,
  })
})

test("liveness answers 200 without the upstream", async ({ request }) => {
  const response = await request.get("/healthz")
  expect(response.status()).toBe(200)
  expect(await response.json()).toEqual({ status: "ok" })
})
