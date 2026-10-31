import type { Page } from "@playwright/test"

/** Reply for one mocked call; status defaults to 200. */
export type MockReply = { status?: number; body?: unknown }

/**
 * Answers the browser's `/v1/*` calls from a table keyed `"METHOD /path"` (path after /v1,
 * query ignored), e.g. `{ "GET /me/workspaces": { body: { data: [] } } }`. For apps that call
 * the API from the browser (office, till, field). Any call not in the table gets a 501, which
 * the test guard reports, so a test never reaches a real backend by accident.
 */
export async function mockApi(page: Page, routes: Record<string, MockReply>) {
  await page.route("**/v1/**", async (route) => {
    const request = route.request()
    const path = new URL(request.url()).pathname.replace(/^\/v1/, "")
    const key = `${request.method()} ${path}`
    const reply = routes[key]
    await route.fulfill({
      status: reply ? (reply.status ?? 200) : 501,
      contentType: "application/json",
      body: JSON.stringify(
        reply ? (reply.body ?? {}) : { message: `not mocked: ${key}` }
      ),
    })
  })
}
