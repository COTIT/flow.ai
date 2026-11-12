import studioEn from "../../../../apps/flow/messages/en/studio.json" with { type: "json" }
import { expectNoHorizontalScroll } from "../../src/test"
import {
  designs,
  expect,
  member,
  previewLinks,
  published,
  root,
  test,
} from "./storefront-api"
import { watch } from "./voice"

const st = studioEn.studio

test("reopening Flow offers to continue the saved draft, from server state, with no voice call", async ({
  page,
  api,
}) => {
  designs(api)
  previewLinks(api, ["active"])
  const sessions = watch(page, "session")
  await page.goto("/")
  const prompt = page.getByRole("region", { name: st.continueTitle })
  await expect(prompt).toContainText("Your new look (v2) is ready to check.")
  await expect(prompt).toContainText("Preview expires")
  await expectNoHorizontalScroll(page)
  expect(sessions).toHaveLength(0)

  await prompt.getByRole("link", { name: st.continue }).click()
  await expect(page).toHaveURL(/\/storefront$/)
  // The token was shown once in another visit: a new link is one click away.
  await expect(page.getByText(st.hiddenNote)).toBeVisible()
})

test("no link yet: the prompt says the draft is saved; Dismiss hides it for this visit", async ({
  page,
  api,
}) => {
  designs(api)
  await page.goto("/")
  const prompt = page.getByRole("region", { name: st.continueTitle })
  await expect(prompt).toContainText(st.continueNoLink)
  await prompt.getByRole("button", { name: st.dismiss }).click()
  await expect(prompt).toBeHidden()
})

test("nothing unpublished: no prompt", async ({ page, api }) => {
  designs(api)
  api.on("GET", root(), () => ({
    data: {
      version: 2,
      draft_revision_id: published,
      published_revision_id: published,
      draft: null,
    },
  }))
  await page.goto("/")
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await expect(page.getByRole("region", { name: st.continueTitle })).toHaveCount(0)
})

test("members never ask for the storefront from home", async ({ page, api }) => {
  designs(api)
  api.on("GET", "/me", () => ({ data: { user: member } }))
  await page.goto("/")
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await page.waitForTimeout(500)
  expect(api.calls.filter((c) => c.path.includes("/storefront/"))).toHaveLength(0)
})
