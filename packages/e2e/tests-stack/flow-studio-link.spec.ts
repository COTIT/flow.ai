import { spawn, type ChildProcess } from "node:child_process"
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

import type { APIRequestContext } from "@playwright/test"

import { apiAs, expect, mailTo, test, unique, urls } from "../src/stack"
import { expectNoHorizontalScroll } from "../src/test"

const root = resolve(import.meta.dirname, "../../..")
const fixture = JSON.parse(
  readFileSync(
    resolve(root, "apps/api/tests/fixtures/storefront-design-v1.json"),
    "utf8"
  )
)

/** One `next start` of an app's stack build on its own port; stderr kept for the startup check. */
function start(app: "shop" | "flow", port: number, env: Record<string, string>) {
  const child = spawn(
    process.execPath,
    [
      resolve(root, `apps/${app}/node_modules/next/dist/bin/next`),
      "start",
      "--port",
      String(port),
      "--hostname",
      "127.0.0.1",
    ],
    {
      cwd: resolve(root, `apps/${app}`),
      env: { ...process.env, NEXT_DIST_DIR: ".next-stack", ...env },
      stdio: ["ignore", "ignore", "pipe"],
    }
  )
  let errors = ""
  child.stderr?.on("data", (chunk: Buffer) => (errors += chunk.toString()))
  return { child, errors: () => errors }
}

async function up(request: APIRequestContext, url: string, what: string) {
  await expect
    .poll(
      async () => {
        try {
          return (await request.get(url)).status()
        } catch {
          return 0
        }
      },
      { timeout: 30_000, message: `${what} starts` }
    )
    .toBe(200)
}

/**
 * Flow's Studio against the real API and a real Shop: issue a link in Flow, open it on the Shop,
 * revoke it in Flow, issue another, publish from Flow's publish card; every link then ends.
 * Own workspace, Shop and Flow processes: the shared seed's storefront stays untouched, and this
 * Flow's SHOP_PUBLIC_URL is that Shop.
 */
test("Flow issues a preview link the Shop opens until revoked; publishing from Flow ends the next one", async ({
  page,
  request,
}, testInfo) => {
  test.setTimeout(150_000)
  const name = `e2e-studio-${unique()}`
  const login = {
    app: "flow (studio)",
    role: "owner",
    login: `${name}@e2e.test`,
    secret: `E2e!${unique()}Aa1`,
    path: "/login",
  }
  const signup = await request.post(`${urls.api}/v1/create-workspace`, {
    data: {
      first_name: "Studio",
      last_name: "Owner",
      email: login.login,
      password: login.secret,
      workspace_name: name,
    },
  })
  expect(signup.status()).toBe(201)
  const verify = (await mailTo(login.login)).match(
    /https?:\/\/\S+\/v1\/verify-email\/\S+/
  )?.[0]
  expect(verify).toBeTruthy()
  await request.get(verify!, { maxRedirects: 0 })
  const owner = await apiAs(request, login)
  const key = await owner.post<{ key: string }>(owner.ws("/app-keys"), {
    name: "Studio test Shop",
    kind: "shop",
    scopes: ["catalog:read"],
  })
  const offset = Number(process.env.E2E_STACK_OFFSET ?? 0) + testInfo.workerIndex
  const shop = `http://127.0.0.1:${4630 + offset}`
  const flow = `http://127.0.0.1:${4640 + offset}`
  const procs: { child: ChildProcess; errors: () => string }[] = [
    start("shop", 4630 + offset, {
      API_URL: `${urls.api}/v1`,
      STOREFRONT_APP_KEY: key.key,
      SITE_URL: shop,
      FLOW_URL: flow,
    }),
    start("flow", 4640 + offset, {
      API_URL: `${urls.api}/v1`,
      VOICE_UPSTREAM_URL: urls.office,
      OFFICE_PUBLIC_URL: urls.office,
      SHOP_PUBLIC_URL: shop,
    }),
  ]
  try {
    await up(request, shop, "isolated Shop")
    await up(request, `${flow}/login`, "isolated Flow")
    const draft = structuredClone(fixture)
    draft.sections[0].props.heading = `Studio draft ${unique()}`
    const saved = await request.post(
      `${urls.api}/v1${owner.ws("/storefront/pages/home")}/revisions`,
      {
        headers: {
          Authorization: `Bearer ${owner.token}`,
          "Idempotency-Key": unique(),
        },
        data: { expected_version: 0, document: draft },
      }
    )
    expect(saved.status(), await saved.text()).toBeLessThan(300)
    const revision = (await saved.json()).data.draft_revision_id as string

    await page.goto(`${flow}/login`)
    await page.getByLabel("Email").fill(login.login)
    await page.getByLabel("Password").fill(login.secret)
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(page).toHaveURL(`${flow}/`)
    await page.goto(`${flow}/storefront`)
    const card = page.getByRole("region", { name: "Redesign" })
    await expect(card.getByText("Your storefront redesign is ready")).toBeVisible()
    await expectNoHorizontalScroll(page)

    // Issue in Flow → the Shop opens the real draft with its banner.
    await card.getByRole("button", { name: "Generate preview link" }).click()
    const open = card.getByRole("link", { name: "Open full-page preview (new tab)" })
    const href = await open.getAttribute("href")
    expect(href).toMatch(new RegExp(`^${shop}/preview/[0-9a-f]{64}$`))
    const [tab] = await Promise.all([page.context().waitForEvent("page"), open.click()])
    await expect(tab).toHaveURL(href!)
    await expect(
      tab.getByRole("status").filter({ hasText: "Unpublished preview" })
    ).toContainText(`Revision ${revision.slice(0, 8)}`)
    await expect(tab.getByRole("heading", { level: 1 })).toHaveText(
      draft.sections[0].props.heading
    )
    const token = href!.split("/").pop()!
    const kept = await page.evaluate(() =>
      JSON.stringify({ ...localStorage, ...sessionStorage }) + document.cookie + location.href
    )
    expect(kept).not.toContain(token)

    // Revoke in Flow → the same Shop URL shows the expired page at once.
    await card.getByRole("button", { name: "Revoke link" }).click()
    await expect(card.getByText("Preview link revoked")).toBeVisible()
    await tab.reload()
    await expect(
      tab.getByRole("heading", { level: 1, name: "This preview has expired" })
    ).toBeVisible()

    // A new link works; publishing from Flow's publish card ends it and makes the draft live.
    await card.getByRole("button", { name: "Generate a new link" }).click()
    const second = await card
      .getByRole("link", { name: "Open full-page preview (new tab)" })
      .getAttribute("href")
    expect(second).not.toBe(href)
    await tab.goto(second!)
    await expect(tab.getByRole("heading", { level: 1 })).toHaveText(
      draft.sections[0].props.heading
    )
    await card.getByRole("button", { name: "Review publication" }).click()
    const publish = page.getByRole("region", { name: "Publish storefront revision?" })
    await publish.getByRole("button", { name: "Confirm publish" }).click()
    await expect(
      page.getByRole("region", { name: "Storefront published" })
    ).toContainText("Verified after publishing")
    await tab.reload()
    await expect(
      tab.getByRole("heading", { level: 1, name: "This preview has expired" })
    ).toBeVisible()
    const state = await owner.get<{ published_revision_id: string }>(
      owner.ws("/storefront/pages/home")
    )
    expect(state.published_revision_id).toBe(revision)
  } finally {
    for (const p of procs) {
      p.child.kill("SIGTERM")
      expect(p.errors()).not.toMatch(/EADDRINUSE|Could not find a production build/)
    }
  }
})
