import studioEn from "../../../../apps/flow/messages/en/studio.json" with { type: "json" }
import { expectNoHorizontalScroll } from "../../src/test"
import {
  designs,
  draft,
  expect,
  previewLinks,
  previewToken,
  published,
  root,
  test,
} from "./storefront-api"
import { fakeAgent } from "./voice"

const st = studioEn.studio

test("the brief sends one request; the draft it produces gets its own link at once", async ({
  page,
  api,
}) => {
  designs(api)
  previewLinks(api)
  // Nothing unpublished yet: the live design is also the draft.
  let current = published
  api.on("GET", root(), () => ({
    data: {
      version: 2,
      draft_revision_id: current,
      published_revision_id: published,
      draft: null,
    },
  }))
  const agent = await fakeAgent(page)
  await page.goto("/storefront")
  const region = page.getByRole("region", { name: st.region })
  await expect(region.getByText("Your shop shows version 1.", { exact: false })).toBeVisible()
  await region.getByRole("button", { name: st.start }).click()

  const brief = page.getByRole("region", { name: st.briefTitle })
  const prepare = brief.getByRole("button", { name: st.prepare })
  await expect(prepare).toBeDisabled()
  await brief.getByRole("button", { name: "Hero" }).click()
  await brief.getByRole("button", { name: "Typography" }).click()
  await expect(brief.getByRole("button", { name: "Hero" })).toHaveAttribute("aria-pressed", "true")
  await brief.getByRole("button", { name: st.change }).click()
  await brief.getByRole("button", { name: st.product }).click()
  await expectNoHorizontalScroll(page)
  await prepare.click()

  await expect(page.getByRole("region", { name: st.preparingTitle })).toContainText(
    "Your shop keeps showing version 1 until you publish."
  )
  await expect
    .poll(() => agent.sent.filter((e) => e.type === "user_message").map((e) => e.text))
    .toEqual([
      "Redesign my storefront: change the hero and typography; the product structure may change; use a product-focused layout.",
    ])
  // Nothing written by the brief itself.
  expect(api.calls.filter((c) => c.method !== "GET")).toHaveLength(0)

  // The assistant's approved edit saved the draft; the studio notices and issues one link.
  current = draft
  await expect(region.getByRole("link", { name: st.openLabel })).toHaveAttribute(
    "href",
    `https://shop.genesy.test/preview/${previewToken(1)}`,
    { timeout: 15_000 }
  )
  expect(api.callsTo("POST", `${root()}/preview-links`)).toHaveLength(1)
})

test("a draft not born from this tab waits for an explicit link", async ({ page, api }) => {
  designs(api)
  previewLinks(api)
  await page.goto("/storefront")
  await expect(
    page.getByRole("button", { name: st.generate })
  ).toBeVisible()
  await page.waitForTimeout(500)
  expect(api.callsTo("POST", `${root()}/preview-links`)).toHaveLength(0)
})
