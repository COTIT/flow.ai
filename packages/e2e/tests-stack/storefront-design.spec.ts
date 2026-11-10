import { spawn } from "node:child_process"
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

import {
  apiAs,
  expect,
  loginOffice,
  mailTo,
  test,
  unique,
  urls,
} from "../src/stack"
import { expectNoHorizontalScroll } from "../src/test"

const root = resolve(import.meta.dirname, "../../..")
const fixture = JSON.parse(
  readFileSync(
    resolve(root, "apps/api/tests/fixtures/storefront-design-v1.json"),
    "utf8"
  )
)

/** Each browser owns its workspace and Shop process; shared seed stays unchanged. */
test("an API draft is private until Office publishes, and restoring requires another publish", async ({
  page,
  request,
}, testInfo) => {
  test.setTimeout(120_000)
  const name = `e2e-design-${unique()}`
  const ownerLogin = {
    app: "office (design)",
    role: "owner",
    login: `${name}@e2e.test`,
    secret: `E2e!${unique()}Aa1`,
    path: "/login",
  }
  const signup = await request.post(`${urls.api}/v1/create-workspace`, {
    data: {
      first_name: "Design",
      last_name: "Owner",
      email: ownerLogin.login,
      password: ownerLogin.secret,
      workspace_name: name,
    },
  })
  expect(signup.status()).toBe(201)
  const link = (await mailTo(ownerLogin.login)).match(
    /https?:\/\/\S+\/v1\/verify-email\/\S+/
  )?.[0]
  expect(link).toBeTruthy()
  expect(
    (await request.get(link!, { maxRedirects: 0 })).headers().location
  ).toContain("email_verified=1")
  const owner = await apiAs(request, ownerLogin)
  const key = await owner.post<{ key: string }>(owner.ws("/app-keys"), {
    name: "Design test Shop",
    kind: "shop",
    scopes: ["catalog:read"],
  })
  await owner.post(owner.ws("/products"), {
    name: `Harvest ${unique()}`,
    price: "4.00",
  })
  const port =
    4600 + Number(process.env.E2E_STACK_OFFSET ?? 0) + testInfo.workerIndex
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
        SHOP_DESIGN_ENABLED: "true",
        SITE_URL: shop,
      },
      stdio: ["ignore", "ignore", "pipe"],
    }
  )
  let startupErrors = ""
  child.stderr?.on("data", (chunk: Buffer) => {
    startupErrors += chunk.toString()
  })
  const path = owner.ws("/storefront/pages/home")
  const write = async (suffix: string, body: unknown) => {
    const response = await request.post(`${urls.api}/v1${path}${suffix}`, {
      headers: {
        Authorization: `Bearer ${owner.token}`,
        "Idempotency-Key": unique(),
      },
      data: body,
    })
    expect(response.ok(), `${suffix}: ${response.status()}`).toBe(true)
    return (await response.json()).data as {
      version: number
      draft_revision_id: string
      published_revision_id: string | null
    }
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
        { timeout: 30_000, message: "isolated design Shop starts" }
      )
      .toBe(200)
    let state = await write("/revisions", {
      expected_version: 0,
      document: fixture,
    })
    const first = state.draft_revision_id
    await page.goto(shop)
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      "Rooted in African soil. Shared with the world."
    )
    await loginOffice(page, ownerLogin)
    await page.goto(`${urls.office}/settings/storefront`)
    await page
      // Revision 1 is the fixture draft (names never carry the UUID).
      .getByRole("button", { name: /^Preview revision 1, saved / })
      .click()
    await expect(
      page.getByRole("region", { name: "Storefront preview" })
    ).toContainText(fixture.sections[0].props.heading)
    await page.getByRole("button", { name: "Publish", exact: true }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Publish", exact: true })
      .click()
    await expect(
      page.getByRole("status").filter({ hasText: "Storefront published." })
    ).toBeVisible()
    await page.goto(shop)
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      fixture.sections[0].props.heading
    )
    await expect(page.getByLabel("Batch code")).toBeVisible()
    await expectNoHorizontalScroll(page)

    state = await owner.get(path)
    const second = structuredClone(fixture)
    second.sections[0].props.heading = `Second harvest ${unique()}`
    state = await write("/revisions", {
      expected_version: state.version,
      document: second,
    })
    state = await write("/publish", {
      expected_version: state.version,
      revision_id: state.draft_revision_id,
    })
    await page.reload()
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      second.sections[0].props.heading
    )

    await page.goto(`${urls.office}/settings/storefront`)
    await page
      // Revision 1 is the fixture draft (names never carry the UUID).
      .getByRole("button", { name: /^Preview revision 1, saved / })
      .click()
    await page
      .getByRole("button", { name: "Restore as draft", exact: true })
      .click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Restore as draft", exact: true })
      .click()
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Revision restored as a new draft." })
    ).toBeVisible()
    state = await owner.get(path)
    expect(state.draft_revision_id).not.toBe(first)
    await page.goto(shop)
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      second.sections[0].props.heading
    )
    await page.goto(`${urls.office}/settings/storefront`)
    await page
      // Revision 3 is the restored copy of revision 1.
      .getByRole("button", { name: /^Preview revision 3, saved / })
      .click()
    await page.getByRole("button", { name: "Publish", exact: true }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Publish", exact: true })
      .click()
    await expect(
      page.getByRole("status").filter({ hasText: "Storefront published." })
    ).toBeVisible()
    await page.goto(shop)
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      fixture.sections[0].props.heading
    )
    await page.screenshot({
      path: testInfo.outputPath("restored-home.png"),
      fullPage: true,
    })
    expect(child.exitCode, startupErrors).toBeNull()
  } finally {
    child.kill("SIGTERM")
  }
})
