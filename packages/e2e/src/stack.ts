import { readFileSync } from "node:fs"
import { join } from "node:path"

import type { APIRequestContext, Page } from "@playwright/test"

import { expect, test as guarded, type Allow } from "./test"

type Credential = {
  app: string
  role: string
  login: string
  secret: string
  path: string
}
type State = {
  urls: Record<"api" | "shop" | "office" | "field" | "till" | "landing", string>
  container: string
}

const dir = join(
  import.meta.dirname,
  "..",
  ".stack",
  process.env.E2E_STACK_OFFSET ?? "0"
)
function load<T>(file: string): T {
  try {
    return JSON.parse(readFileSync(join(dir, file), "utf8")) as T
  } catch {
    throw new Error(
      `no full stack at ${dir}: run \`pnpm --filter @genesy/e2e stack:up\` first`
    )
  }
}
const state = load<State>("state.json")
const seed = load<{
  credentials: Credential[]
  notes: Record<string, string>
  workspaces: Record<"AFM" | "RUAHA", string>
}>("seed.json")

/** Where each app of the running stack answers, e.g. `stack.urls.office`. */
export const urls = state.urls

/** This stack's Postgres container (`docker exec` it only for the test's own rows). */
export const container = state.container

/**
 * A seeded login (from what `seed-demo` printed), e.g. `cred("office (AFM)", "owner")`,
 * `cred("field (RUAHA)", "ops")`, `cred("till (AFM)", "cashier PIN")`. First match wins.
 */
export function cred(app: string, role: string): Credential {
  const found = seed.credentials.find((c) => c.app === app && c.role === role)
  if (!found)
    throw new Error(
      `no seeded ${role} for ${app}; seeded: ${seed.credentials.map((c) => `${c.app}/${c.role}`).join(", ")}`
    )
  return found
}

/** Each seeded account's workspace id, e.g. `workspaces.AFM`. */
export const workspaces = seed.workspaces

/** The seed's notes (storefront app key, passport path, sample paths), by their printed label. */
export const notes = seed.notes

/** A spec-unique suffix for names of things a test creates (data rule 1 in STACK.md). */
export const unique = () =>
  `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`

/** The API as one member: `get/post/put` on `/v1/...` paths, JSON in and out, throws on >= 400. */
export type Api = Awaited<ReturnType<typeof apiAs>>

/** Logs in through `POST /v1/login` and returns a client for that member's first workspace. */
export async function apiAs(request: APIRequestContext, who: Credential) {
  const r = await request.post(`${urls.api}/v1/login`, {
    data: { email: who.login, password: who.secret },
  })
  expect(r.status(), `login ${who.login}`).toBe(200)
  const { data } = (await r.json()) as {
    data: { access_token: string; workspace_ids: string[] }
  }
  const headers = { Authorization: `Bearer ${data.access_token}` }
  const call = async <T>(
    method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
    path: string,
    body?: unknown
  ) => {
    const res = await request.fetch(`${urls.api}/v1${path}`, {
      method,
      headers,
      data: body,
    })
    if (res.status() >= 400)
      throw new Error(`${method} ${path}: ${res.status()} ${await res.text()}`)
    return (
      res.status() === 204
        ? undefined
        : ((await res.json()) as { data: T }).data
    ) as T
  }
  return {
    token: data.access_token,
    workspace: data.workspace_ids[0]!,
    /** `ws("/products")` is `/workspace/<id>/products`. */
    ws: (path: string) => `/workspace/${data.workspace_ids[0]}${path}`,
    get: <T>(path: string) => call<T>("GET", path),
    post: <T>(path: string, body?: unknown) => call<T>("POST", path, body),
    put: <T>(path: string, body?: unknown) => call<T>("PUT", path, body),
    patch: <T>(path: string, body?: unknown) => call<T>("PATCH", path, body),
  }
}

/** Logs into the office through its login page and waits for the overview. */
export async function loginOffice(page: Page, who: Credential) {
  await page.goto(`${urls.office}/login`)
  await page.getByLabel("Email").fill(who.login)
  await page.getByLabel("Password").fill(who.secret)
  await page.getByRole("button", { name: "Log in" }).click()
  await expect(page).toHaveURL(`${urls.office}/`)
}

/** Logs into Field (staff by email, farmers by phone) through its login page. */
export async function loginField(page: Page, who: Credential) {
  await page.goto(`${urls.field}/login`)
  await page.getByLabel("Phone number or email").fill(who.login)
  await page.getByLabel("Password").fill(who.secret)
  await page.getByRole("button", { name: "Sign in" }).click()
  await expect(page).not.toHaveURL(/\/login$/)
}

type Register = {
  id: string
  name: string
  warehouse_id: string
  currency_id: string
}

/**
 * A till of its own: the AFM owner creates a register (a copy of the seeded counter's warehouse and
 * currency, so pairing never kicks another test's till out), asks for a real pairing code, and the
 * till pairs through /setup. Then the seeded cashier logs in with the PIN pad.
 */
export async function pairTill(page: Page, request: APIRequestContext) {
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  // The seeded counter (first device key printed): the AFM store, where the shop's stock is.
  const counter = cred("till (AFM)", "device key").login
  const seeded = (
    await owner.get<Register[]>(owner.ws("/till/registers"))
  ).find((r) => r.name === counter)
  const register = await owner.post<Register>(owner.ws("/till/registers"), {
    name: `e2e till ${unique()}`,
    warehouse_id: seeded!.warehouse_id,
    currency_id: seeded!.currency_id,
  })
  const pairing = await owner.post<{ code: string; workspace: string }>(
    owner.ws(`/till/registers/${register.id}/pairing`)
  )
  await page.goto(`${urls.till}/setup`)
  await page.getByLabel("Account").fill(pairing.workspace)
  await page.getByLabel("Pairing code").fill(pairing.code)
  await page.getByRole("button", { name: "Pair this till" }).click()
  await expect(page).toHaveURL(`${urls.till}/login`)

  const cashier = cred("till (AFM)", "cashier PIN")
  await page.getByRole("button", { name: cashier.login }).click()
  for (const digit of cashier.secret)
    await page.getByRole("button", { name: digit, exact: true }).click()
  await page.getByRole("button", { name: "Log in" }).click()
  await expect(page).not.toHaveURL(`${urls.till}/login`)
  return { owner, register }
}

/** Opens the paired till's shift with an empty float (a new register has none open; sales need one). */
export async function openShift(page: Page) {
  await page.goto(`${urls.till}/drawer`)
  await page.getByRole("button", { name: "Open shift" }).click()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Shift opened. You can start selling." })
  ).toBeVisible()
}

/**
 * What every stack test allows: the stack runs without a file bucket (STACK.md), so the office's
 * file list answers 503 "File storage isn't configured." A test with its own `allow` spreads
 * this in: `test.use({ allow: { ...stackAllow, responses: [...stackAllow.responses, …] } })`.
 */
export const stackAllow = {
  responses: Object.values(seed.workspaces).map((ws) => ({
    url: `${urls.office}/v1/workspace/${ws}/files?page_size=100`,
    status: 503,
  })),
} satisfies Allow

/**
 * Playwright's test for stack specs: every page fails on console errors, page errors, failed
 * requests and responses >= 400 (the `guard` from ./test), unless allowed (`stackAllow` by
 * default). Allowed URLs must be absolute here (there is no baseURL).
 */
export const test = guarded.extend({ allow: stackAllow })
export { expect }
