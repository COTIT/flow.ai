import { randomUUID } from "node:crypto"

import type { APIRequestContext } from "@playwright/test"

import { cred, expect, loginFlow, test, urls, workspaces } from "../src/stack"

// The stack runs Office's voice server switched off (VOICE=1 needs paid provider keys), so
// readiness is 200 { enabled: false, ready: true }: Flow must show exactly what Office answers.
const owner = cred("office (AFM)", "owner")

async function bearer(request: APIRequestContext) {
  const r = await request.post(`${urls.api}/v1/login`, {
    data: { email: owner.login, password: owner.secret },
  })
  expect(r.status()).toBe(200)
  return `Bearer ${(await r.json()).data.access_token}`
}

/** The same request to Flow's proxy and to Office directly: answers must match. */
async function both(
  request: APIRequestContext,
  path: string,
  init: Parameters<APIRequestContext["fetch"]>[1]
) {
  const [flow, office] = await Promise.all([
    request.fetch(`${urls.flow}${path}`, init),
    request.fetch(`${urls.office}${path}`, init),
  ])
  const answer = async (r: typeof flow) => ({
    status: r.status(),
    body: await r.json(),
  })
  return { flow: await answer(flow), office: await answer(office) }
}

test("a real Flow login shows the member, the workspace and Office's assistant state", async ({
  page,
  request,
}) => {
  const headers = { Authorization: await bearer(request) }
  const mine = await request.get(`${urls.api}/v1/me/workspaces`, { headers })
  const afm = (
    (await mine.json()).data as { id: string; display_name: string }[]
  ).find((w) => w.id === workspaces.AFM)!
  const me = await request.get(`${urls.api}/v1/me`, { headers })
  const { name } = (await me.json()).data.user as { name: string }
  await loginFlow(page, owner)
  // The conversation header names the member (not the login email) and the workspace.
  await expect(page.getByRole("main").getByText(name)).toBeVisible()
  await expect(page.getByRole("main").getByText(afm.display_name)).toBeVisible()
  await expect(
    page.getByRole("status").filter({ hasText: "The assistant is turned off" })
  ).toBeVisible()
})

test("readiness through Flow equals Office's own answer", async ({
  request,
}) => {
  const { flow, office } = await both(request, "/api/voice/readiness", {
    method: "GET",
  })
  expect(flow).toEqual(office)
  expect(flow.status).toBe(200)
  expect(flow.body).toEqual({ enabled: false, ready: true })
})

test("an authenticated voice call reaches Office with its bearer and workspace intact", async ({
  request,
}) => {
  const auth = await bearer(request)
  const end = (authorization: string, workspaceId: string) =>
    both(request, "/api/voice/session", {
      method: "DELETE",
      headers: {
        Authorization: authorization,
        "Content-Type": "application/json",
      },
      data: { workspaceId, sessionId: randomUUID() },
    })

  // The API refuses the token: 401 as Office says it.
  const refused = await end("Bearer not-a-real-token", workspaces.AFM)
  expect(refused.flow).toEqual(refused.office)
  expect(refused.flow.status).toBe(401)

  // A real member, another account's workspace: Office's 403.
  const other = await end(auth, workspaces.RUAHA)
  expect(other.flow).toEqual(other.office)
  expect(other.flow.status).toBe(403)

  // Own workspace, a session that does not exist: whatever Office answers, Flow answers the same.
  const own = await end(auth, workspaces.AFM)
  expect(own.flow).toEqual(own.office)
  expect([401, 403]).not.toContain(own.flow.status)
})
