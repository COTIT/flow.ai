import { spawn } from "node:child_process"
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

import { apiAs, expect, mailTo, test, unique, urls } from "../src/stack"
import { expectNoHorizontalScroll } from "../src/test"

const root = resolve(import.meta.dirname, "../../..")
const fixture = JSON.parse(
  readFileSync(
    resolve(root, "apps/api/tests/fixtures/storefront-design-v1.json"),
    "utf8"
  )
)

/** Own workspace and Shop process: the shared seed's storefront stays untouched. */
test("a preview link opens the draft on the Shop until revoked, and publishing ends a fresh one", async ({
  page,
  request,
}, testInfo) => {
  test.setTimeout(120_000)
  const name = `e2e-preview-${unique()}`
  const login = {
    app: "office (preview)",
    role: "owner",
    login: `${name}@e2e.test`,
    secret: `E2e!${unique()}Aa1`,
    path: "/login",
  }
  const signup = await request.post(`${urls.api}/v1/create-workspace`, {
    data: {
      first_name: "Preview",
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
    name: "Preview test Shop",
    kind: "shop",
    scopes: ["catalog:read"],
  })
  await owner.post(owner.ws("/products"), {
    name: `Harvest ${unique()}`,
    price: "4.00",
  })

  const port =
    4620 + Number(process.env.E2E_STACK_OFFSET ?? 0) + testInfo.workerIndex
  const shop = `http://127.0.0.1:${port}`
  const child = spawn(
    process.execPath,
    [
      resolve(root, "apps/shop/node_modules/next/dist/bin/next"),
      "start",
      "--port",
      String(port),
      "--hostname",
      "127.0.0.1",
    ],
    {
      cwd: resolve(root, "apps/shop"),
      env: {
        ...process.env,
        NEXT_DIST_DIR: ".next-stack",
        API_URL: `${urls.api}/v1`,
        STOREFRONT_APP_KEY: key.key,
        SITE_URL: shop,
        FLOW_URL: urls.flow,
      },
      stdio: ["ignore", "ignore", "pipe"],
    }
  )
  let startupErrors = ""
  child.stderr?.on("data", (chunk: Buffer) => {
    startupErrors += chunk.toString()
  })
  const home = owner.ws("/storefront/pages/home")
  const keyed = async <T>(path: string, body: unknown) => {
    const response = await request.post(`${urls.api}/v1${home}${path}`, {
      headers: {
        Authorization: `Bearer ${owner.token}`,
        "Idempotency-Key": unique(),
      },
      data: body,
    })
    expect(response.status(), `${path}: ${await response.text()}`).toBeLessThan(
      300
    )
    return (await response.json()).data as T
  }
  const issue = (revision: string) =>
    keyed<{ link_id: string; token: string; expires_at: string }>(
      "/preview-links",
      { revision_id: revision }
    )
  const expectExpired = async () => {
    await expect(
      page.getByRole("heading", { level: 1, name: "This preview has expired" })
    ).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Return to Genesy Flow" })
    ).toHaveAttribute("href", urls.flow)
    await expect(page.getByText("Unpublished preview")).toHaveCount(0)
  }
  try {
    await expect
      .poll(
        async () => {
          try {
            return (await request.get(shop)).status()
          } catch {
            return 0
          }
        },
        { timeout: 30_000, message: "isolated preview Shop starts" }
      )
      .toBe(200)
    const draft = structuredClone(fixture)
    draft.sections[0].props.heading = `Draft only ${unique()}`
    const state = await keyed<{ version: number; draft_revision_id: string }>(
      "/revisions",
      { expected_version: 0, document: draft }
    )
    const revision = state.draft_revision_id

    // Issue → open: the real draft with the banner, the live home unchanged.
    const first = await issue(revision)
    expect(first.token).toMatch(/^[0-9a-f]{64}$/)
    const url = `${shop}/preview/${first.token}`
    const response = await page.goto(url)
    expect(response!.headers()["referrer-policy"]).toBe("no-referrer")
    await expect(
      page.getByRole("status").filter({ hasText: "Unpublished preview" })
    ).toContainText(`Revision ${revision.slice(0, 8)}`)
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      draft.sections[0].props.heading
    )
    await expectNoHorizontalScroll(page)

    // Revoke → the same URL shows the expired page at once.
    const revoke = await request.delete(
      `${urls.api}/v1${home}/preview-links/${first.link_id}`,
      { headers: { Authorization: `Bearer ${owner.token}` } }
    )
    expect(revoke.status()).toBe(200)
    await page.reload()
    await expectExpired()

    // A fresh link works again; publishing the draft ends it.
    const second = await issue(revision)
    await page.goto(`${shop}/preview/${second.token}`)
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      draft.sections[0].props.heading
    )
    await keyed("/publish", {
      expected_version: state.version,
      revision_id: revision,
    })
    await page.reload()
    await expectExpired()

    const links = await owner.get<{ link_id: string; status: string }[]>(
      `${home}/preview-links?revision_id=${revision}`
    )
    expect(links.map((l) => [l.link_id, l.status])).toEqual([
      [second.link_id, "ended"],
      [first.link_id, "revoked"],
    ])
  } finally {
    child.kill("SIGTERM")
    expect(startupErrors).not.toMatch(
      /EADDRINUSE|Could not find a production build/
    )
  }
})
