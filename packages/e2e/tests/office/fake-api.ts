import type { Page, Route } from "@playwright/test"

import type { MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import {
  test as base,
  expect,
  expectNoHorizontalScroll,
  expectTapTargets,
} from "../../src/test"

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
  readonly calls: {
    method: string
    path: string
    body: unknown
    headers: Record<string, string>
  }[] = []
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
    this.calls.push({ method, path, body, headers: req.headers() })
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

/** The signed-in user the shell shows. */
export const me = {
  id: "u1",
  name: "Romeo Tester",
  initials: "RT",
  // The creator of both workspaces: every permission in each.
  permissions: [
    { workspace: WS, permissions: ["workspace_owner"] },
    { workspace: WS2, permissions: ["workspace_owner"] },
  ],
}

/** Installs a FakeApi on the page with the calls every signed-in screen makes. */
export async function fakeApi(page: Page) {
  const api = new FakeApi()
    .on("GET", "/me/workspaces", () => ({
      data: [
        // The slug and the company name: the app shows display_name.
        { id: WS, workspace_name: "ilundo-honey", display_name: "Ilundo Honey Co." },
        { id: WS2, workspace_name: "other-co", display_name: "Other Co" },
      ],
    }))
    .on("POST", "/logout", () => ({ status: 204 }))
    // The shell: the signed-in user's tile, the open-orders and pending-links counts in the menu.
    .on("GET", "/me", () => ({ data: { user: me } }))
    .on("GET", "/workspace/:ws/orders", () => paged([]))
    .on("GET", "/workspace/:ws/supply-links", () => paged([]))
    // Every order page reads its trail; older orders have none.
    .on("GET", "/workspace/:ws/orders/:order/events", () => paged([]))
    // Every product page lists its sizes; specs that care override this.
    .on("GET", "/workspace/:ws/products/:id/variants", () => ({ data: [] }))
    // Every batch page reads its farmer credits; specs that care override this.
    .on("GET", "/workspace/:ws/lots/:lot/farmer-credits", () => ({
      ...paged([]),
      farmer_groups: null,
      farmers: [],
    }))
    // Every batch page reads the orders still holding it; specs that care override this.
    .on("GET", "/workspace/:ws/lots/:lot/holds", () => ({ data: [] }))
    // Product pages, the list and New product read the categories; specs that care override this.
    .on("GET", "/workspace/:ws/erp/product-categories", () => paged([], 1, 100))
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

/** On a phone the menu is a sheet behind "Open menu"; this opens it (no-op on a desktop). */
export async function openMenu(page: Page) {
  // By width, not by whether the button is drawn yet: a slow first paint skipped the click.
  if ((page.viewportSize()?.width ?? 0) >= 768) return
  const nav = page.getByRole("navigation", { name: "Main menu" })
  // Already open (e.g. still open after picking a workspace).
  if (await nav.isVisible()) return
  await page.getByRole("button", { name: "Open menu" }).click()
  await expect(nav).toBeVisible()
}

/** Picks `option` in the shadcn Select named `name` (a combobox button, not a native select). */
export async function choose(page: Page, name: string, option: string) {
  await page.getByRole("combobox", { name }).click()
  await page.getByRole("option", { name: option, exact: true }).click()
}

/** The page fits the screen (no sideways scroll) and, on a phone, every control is a 44 px target. */
export async function expectFits(page: Page) {
  // Measure the settled page, not a dialog half way through closing.
  // A cancelled animation rejects `finished` with AbortError; it has settled all the same.
  await page.evaluate(() =>
    Promise.all(
      document.getAnimations().map((a) => a.finished.catch(() => undefined))
    )
  )
  await expectNoHorizontalScroll(page)
  if ((page.viewportSize()?.width ?? 0) < 768) await expectTapTargets(page)
}

export { expect }
