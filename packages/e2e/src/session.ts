import type { Page } from "@playwright/test"

/**
 * Starts the page logged in: writes the `genesy.session` entry @genesy/api-client keeps in
 * localStorage, before any app script runs. Pair with mockApi so the token is never sent anywhere real.
 */
export async function seedSession(
  page: Page,
  {
    token = "e2e-token",
    workspaceIds = ["w1"],
  }: { token?: string; workspaceIds?: string[] } = {}
) {
  const value = JSON.stringify({
    state: { token, workspaceIds, workspaceId: workspaceIds[0] ?? null },
    version: 0,
  })
  await page.addInitScript(
    (v) => localStorage.setItem("genesy.session", v),
    value
  )
}
