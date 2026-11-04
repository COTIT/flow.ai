import type { APIRequestContext, Page } from "@playwright/test"

import { cred, expect, loginField, urls, workspaces } from "../../src/stack"

type Who = ReturnType<typeof cred>
type Me = { user: { id: string; name: string }; memberships: { village_id: string | null; person_id: string | null }[] }

/** A Field session's token: staff sign in by email, farmers by phone. */
async function token(request: APIRequestContext, who: Who) {
  const farmer = who.role === "farmer"
  const r = await request.post(`${urls.api}/v1/${farmer ? "farm/login" : "login"}`, {
    data: farmer ? { phone: who.login, password: who.secret } : { email: who.login, password: who.secret },
  })
  expect(r.status(), `login ${who.login}`).toBe(200)
  return ((await r.json()) as { data: { access_token: string } }).data.access_token
}

/** `GET/POST /v1/workspace/<RUAHA>/…` as this Field user; `status` keeps a refusal instead of throwing. */
export async function farmApi(request: APIRequestContext, who: Who) {
  const headers = { Authorization: `Bearer ${await token(request, who)}` }
  const me = ((await (await request.get(`${urls.api}/v1/farm/me`, { headers })).json()) as { data: Me }).data
  const call = async <T>(method: "GET" | "POST", path: string, body?: unknown) => {
    const res = await request.fetch(`${urls.api}/v1/workspace/${workspaces.RUAHA}${path}`, { method, headers, data: body })
    const json = res.status() === 204 ? {} : ((await res.json()) as Record<string, unknown>)
    return { status: res.status(), body: json as T & { data: unknown } }
  }
  const ok = async <T>(method: "GET" | "POST", path: string, body?: unknown) => {
    const r = await call<{ data: T }>(method, path, body)
    if (r.status >= 400) throw new Error(`${method} ${path}: ${r.status} ${JSON.stringify(r.body)}`)
    return r.body as { data: T } & Record<string, unknown>
  }
  return { me, call, get: <T>(path: string) => ok<T>("GET", path), post: <T>(path: string, body?: unknown) => ok<T>("POST", path, body) }
}

/**
 * Signs into Field through its login page. The first-visit guided tour is marked seen for this
 * user first: it would take them to their home mid-test (it has its own fake-API spec).
 */
export async function signIn(page: Page, request: APIRequestContext, who: Who) {
  const { me } = await farmApi(request, who)
  await page.addInitScript(
    (seen) => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(seen)),
    ["farmer", "officer", "ops"].map((s) => `${s}:${me.user.id}`),
  )
  await loginField(page, who)
  return me
}

/** A seeded Field login by role and address, e.g. `who("field_officer", "juma@ruaha-demo.test")`. */
export function who(role: string, login: string): Who {
  return { ...cred("field (RUAHA)", role), login, secret: cred("field (RUAHA)", role).secret }
}

export type PassbookLine = { id: string; kind: string; amount: { minor: number; currency: string }; payout_id: string | null }
export type Passbook = { data: PassbookLine[]; balances: { minor: number; currency: string; currency_id: string }[] }
