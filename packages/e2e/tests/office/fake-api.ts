import type { Page, Route } from "@playwright/test"

import type { MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { test as base, expect } from "../../src/test"

type Handler = (req: {
  params: string[]
  query: URLSearchParams
  body: unknown
}) => MockReply | unknown

/**
 * A stateful stand-in for the API, for flows a fixed mockApi table can't follow
 * (create → list, publish → version 1). Routes take `:param` paths; handlers return
 * a body, or `{ status, body }`. Unknown calls get a 501, which the test guard reports.
 * Every call is recorded, so a test can prove what was (or wasn't) sent.
 */
export class FakeApi {
  readonly calls: { method: string; path: string; body: unknown }[] = []
  private routes: { method: string; re: RegExp; handler: Handler }[] = []

  on(method: string, path: string, handler: Handler) {
    const re = new RegExp(`^${path.replace(/:[a-z_]+/g, "([^/]+)")}$`)
    // Newest first, so a test can override a default route.
    this.routes.unshift({ method, re, handler })
    return this
  }

  /** Calls made to `method path` (`:param` matches any one segment). */
  callsTo(method: string, path: string) {
    const re = new RegExp(`^${path.replace(/:[a-z_]+/g, "[^/]+")}$`)
    return this.calls.filter((c) => c.method === method && re.test(c.path))
  }

  async handle(route: Route) {
    const req = route.request()
    const url = new URL(req.url())
    const path = url.pathname.replace(/^\/v1/, "")
    const method = req.method()
    const body = req.postData() ? req.postDataJSON() : undefined
    this.calls.push({ method, path, body })
    for (const r of this.routes) {
      const m = r.method === method && path.match(r.re)
      if (!m) continue
      const out = r.handler({
        params: m.slice(1),
        query: url.searchParams,
        body,
      })
      const reply =
        out && typeof out === "object" && "status" in out
          ? (out as MockReply)
          : { status: 200, body: out }
      return route.fulfill({
        status: reply.status ?? 200,
        contentType: "application/json",
        body: reply.body === undefined ? "" : JSON.stringify(reply.body),
      })
    }
    return route.fulfill({ status: 501, body: `not faked: ${method} ${path}` })
  }
}

/** Two workspaces, so specs can prove a switch refetches for the other one. */
export const WS = "w1"
export const WS2 = "w2"

/** Installs a FakeApi on the page with the calls every signed-in screen makes. */
export async function fakeApi(page: Page) {
  const api = new FakeApi()
    .on("GET", "/me/workspaces", () => ({
      data: [
        { id: WS, workspace_name: "Ilundo Honey Co." },
        { id: WS2, workspace_name: "Other Co" },
      ],
    }))
    .on("POST", "/logout", () => ({ status: 204 }))
  await page.route("**/v1/**", (route) => api.handle(route))
  return api
}

/** A paged reply shaped like the API's `Paginated_*`. */
export function paged<T>(rows: T[], page = 1, perPage = 50) {
  const start = (page - 1) * perPage
  const data = rows.slice(start, start + perPage)
  return {
    data,
    meta: {
      current_page: page,
      last_page: Math.max(1, Math.ceil(rows.length / perPage)),
      per_page: perPage,
      total: rows.length,
      from: data.length ? start + 1 : null,
      to: data.length ? start + data.length : null,
    },
  }
}

/** The shared guarded `test`, signed in to WS (WS2 also allowed), with `api` faked. */
export const test = base.extend<{ api: FakeApi }>({
  api: async ({ page }, use) => {
    await seedSession(page, { workspaceIds: [WS, WS2] })
    await use(await fakeApi(page))
  },
})

export { expect }
