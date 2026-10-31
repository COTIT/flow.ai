import { test as base, expect, type Page, type Route } from "@playwright/test"

type Json = unknown
type Reply = { status?: number; body?: Json }
type Handler = (req: {
  params: string[]
  query: URLSearchParams
  body: Json
}) => Reply | Json

/**
 * In-browser stand-in for the Genesy API: tests register routes like
 * `api.on("GET", "/workspace/:ws/products", ...)` and keep their own state.
 * Any unregistered call fails the test.
 */
export class FakeApi {
  readonly calls: { method: string; path: string; body: Json }[] = []
  readonly unhandled: string[] = []
  private routes: { method: string; re: RegExp; handler: Handler }[] = []

  on(method: string, path: string, handler: Handler) {
    const re = new RegExp(`^${path.replace(/:[a-z_]+/g, "([^/]+)")}$`)
    // Newest first, so a test can override a default route.
    this.routes.unshift({ method, re, handler })
    return this
  }

  /** Calls made to `method path` (path as a regexp source). */
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
          ? (out as Reply)
          : { status: 200, body: out }
      return route.fulfill({
        status: reply.status ?? 200,
        contentType: "application/json",
        // Marks deliberate error replies so the guard below lets them through.
        headers: { "x-fake-api": "1" },
        body: reply.body === undefined ? "" : JSON.stringify(reply.body),
      })
    }
    this.unhandled.push(`${method} ${path}`)
    return route.fulfill({ status: 501, headers: { "x-fake-api": "1" } })
  }
}

export const WS = "01930000-0000-7000-8000-000000000001"
export const WS2 = "01930000-0000-7000-8000-000000000002"

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

async function signIn(page: Page) {
  await page.addInitScript(
    ([ws, ws2]) =>
      localStorage.setItem(
        "genesy.session",
        JSON.stringify({
          state: {
            token: "test-token",
            workspaceIds: [ws, ws2],
            workspaceId: ws,
          },
          version: 0,
        })
      ),
    [WS, WS2]
  )
}

export const test = base.extend<{ api: FakeApi; guard: void }>({
  api: async ({ page }, use) => {
    const api = new FakeApi()
    api
      .on("GET", "/me/workspaces", () => ({
        data: [
          { id: WS, workspace_name: "Ilundo Honey" },
          { id: WS2, workspace_name: "Other Co" },
        ],
      }))
      .on("POST", "/logout", () => ({ status: 204 }))
    await page.route("**/v1/**", (route) => api.handle(route))
    await signIn(page)
    await use(api)
    expect(api.unhandled, "API calls the fake does not know").toEqual([])
  },
  // Every test: no console errors, no failed or unexpected error requests.
  guard: [
    async ({ page }, use) => {
      const problems: string[] = []
      page.on("pageerror", (e) => problems.push(`pageerror: ${e.message}`))
      page.on("console", (m) => {
        // Chrome logs every 4xx/5xx as a console error; those are judged below.
        if (
          m.type() === "error" &&
          !m.text().startsWith("Failed to load resource")
        )
          problems.push(`console: ${m.text()}`)
      })
      page.on("requestfailed", (r) => {
        // Navigating away aborts in-flight requests; that is not a failure.
        if (r.failure()?.errorText !== "net::ERR_ABORTED")
          problems.push(`requestfailed: ${r.url()} ${r.failure()?.errorText}`)
      })
      page.on("response", async (r) => {
        if (r.status() >= 400 && !(await r.headerValue("x-fake-api")))
          problems.push(`http ${r.status()}: ${r.url()}`)
      })
      await use()
      expect(problems).toEqual([])
    },
    { auto: true },
  ],
})

export { expect }
