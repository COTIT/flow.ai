import type { Page } from "@playwright/test"

import studioEn from "../../../../apps/flow/messages/en/studio.json" with { type: "json" }
import { expectNoHorizontalScroll } from "../../src/test"
import {
  designs,
  draft,
  expect,
  member,
  previewLinks,
  previewToken,
  root,
  test,
} from "./storefront-api"

const st = studioEn.studio
const SHOP = "https://shop.genesy.test"
const card = (page: Page) => page.getByRole("region", { name: st.region })

/** The token is a bearer credential: it must never reach storage, cookies or Flow's own URL. */
async function expectTokenNowhere(page: Page, token: string) {
  const places = await page.evaluate(() => ({
    local: JSON.stringify({ ...localStorage }),
    session: JSON.stringify({ ...sessionStorage }),
    cookie: document.cookie,
    url: location.href,
  }))
  for (const [place, text] of Object.entries(places))
    expect(text, place).not.toContain(token)
}

test("issue a link, open it in a new tab, revoke it; the token stays out of storage and logs", async ({
  page,
  api,
}) => {
  designs(api)
  previewLinks(api)
  const logs: string[] = []
  page.on("console", (m) => logs.push(m.text()))
  await page.goto("/storefront")
  const region = card(page)
  await expect(region.getByText(st.readyTitle)).toBeVisible()
  await expect(region.getByText("version 2 · your shop still shows version 1")).toBeVisible()
  await expect(region.getByText(st.unpublished)).toBeVisible()
  await expect(region.getByText(st.accessNone)).toBeVisible()
  await expect(region.getByRole("link", { name: st.openLabel })).toHaveCount(0)

  await region.getByRole("button", { name: st.generate }).click()
  const open = region.getByRole("link", { name: st.openLabel })
  await expect(open).toHaveAttribute("href", `${SHOP}/preview/${previewToken(1)}`)
  await expect(open).toHaveAttribute("target", "_blank")
  await expect(open).toHaveAttribute("rel", "noopener noreferrer")
  await expect(region.getByText(st.accessActive)).toBeVisible()
  const issued = api.callsTo("POST", `${root()}/preview-links`)
  expect(issued).toHaveLength(1)
  expect(issued[0]?.body).toEqual({ revision_id: draft })
  expect(issued[0]?.headers["idempotency-key"]).toBeTruthy()
  await expectNoHorizontalScroll(page)

  // The Shop is another origin; here it is a stub page.
  await page.context().route(`${SHOP}/**`, (route) =>
    route.fulfill({ contentType: "text/html", body: "<title>Preview</title>" })
  )
  const [tab] = await Promise.all([page.context().waitForEvent("page"), open.click()])
  await expect(tab).toHaveURL(`${SHOP}/preview/${previewToken(1)}`)
  await tab.close()

  await expectTokenNowhere(page, previewToken(1))
  expect(logs.join("\n")).not.toContain(previewToken(1))

  await region.getByRole("button", { name: st.revoke }).click()
  await expect(region.getByText(st.revokedTitle)).toBeVisible()
  await expect(region.getByText(st.accessRevoked)).toBeVisible()
  await expect(open).toHaveCount(0)
  expect(api.callsTo("DELETE", `${root()}/preview-links/link-1`)).toHaveLength(1)
  await expect(region.getByRole("button", { name: st.regenerate })).toBeVisible()
})

test("an expired link offers a new one; the newest link decides", async ({ page, api }) => {
  designs(api)
  previewLinks(api, ["expired"])
  await page.goto("/storefront")
  const region = card(page)
  await expect(region.getByText(st.expiredTitle)).toBeVisible()
  await expect(region.getByText(st.accessExpired)).toBeVisible()
  await region.getByRole("button", { name: st.regenerate }).click()
  await expect(region.getByRole("link", { name: st.openLabel })).toHaveAttribute(
    "href",
    `${SHOP}/preview/${previewToken(1)}`
  )
  await expect(region.getByText(st.readyTitle)).toBeVisible()
})

test("after a reload an active link cannot be opened here: shown once, generate a new one", async ({
  page,
  api,
}) => {
  designs(api)
  previewLinks(api)
  await page.goto("/storefront")
  await card(page).getByRole("button", { name: st.generate }).click()
  await expect(card(page).getByRole("link", { name: st.openLabel })).toBeVisible()
  await page.reload()
  const region = card(page)
  await expect(region.getByText(st.hiddenNote)).toBeVisible()
  await expect(region.getByText(st.accessHidden)).toBeVisible()
  await expect(region.getByRole("link", { name: st.openLabel })).toHaveCount(0)
  await region.getByRole("button", { name: st.regenerate }).click()
  await expect(region.getByRole("link", { name: st.openLabel })).toHaveAttribute(
    "href",
    `${SHOP}/preview/${previewToken(2)}`
  )
})

test("publication needs its own card: live v1 → proposed v2, no preview tab while it is open", async ({
  page,
  api,
}) => {
  designs(api)
  const links = previewLinks(api)
  await page.goto("/storefront")
  const region = card(page)
  await region.getByRole("button", { name: st.generate }).click()
  await expect(region.getByRole("link", { name: st.openLabel })).toBeVisible()

  await region.getByRole("button", { name: st.review }).click()
  const publish = page.getByRole("region", { name: st.publishTitle })
  await expect(publish.getByText(st.publishTitle)).toBeVisible()
  await expect(publish.getByText(st.currentLive)).toBeVisible()
  await expect(publish.locator("dd").nth(0)).toHaveText("version 1")
  await expect(publish.locator("dd").nth(1)).toHaveText("version 2")
  await expect(publish.locator("dd").nth(2)).toHaveText("Ilundo Honey Co. storefront")
  await expect(publish.getByText(st.replace)).toBeVisible()
  // Opening a tab hides this one, which cancels open reviews: no link while deciding.
  await expect(page.getByRole("link", { name: st.openLabel })).toHaveCount(0)
  await expectNoHorizontalScroll(page)

  await publish.getByRole("button", { name: st.cancel }).click()
  await expect(publish).toBeHidden()
  expect(api.callsTo("POST", `${root()}/publish`)).toHaveLength(0)

  await region.getByRole("button", { name: st.review }).click()
  links.set("ended")
  await page
    .getByRole("region", { name: st.publishTitle })
    .getByRole("button", { name: st.confirm })
    .click()
  const receipt = page.getByRole("region", { name: st.publishedTitle })
  await expect(
    receipt.getByText(
      "Your shop now shows version 2. The earlier look (version 1) is kept and can be brought back later."
    )
  ).toBeVisible()
  const calls = api.callsTo("POST", `${root()}/publish`)
  expect(calls).toHaveLength(1)
  expect(calls[0]?.body).toEqual({ expected_version: 2, revision_id: draft })
  expect(calls[0]?.headers["idempotency-key"]).toBeTruthy()
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/storefront/pages/home/publish", status: 409 },
      ],
    },
  })

  test("a 409 on publish shows the conflict and never retries", async ({ page, api }) => {
    designs(api)
    previewLinks(api)
    api.on("POST", `${root()}/publish`, () => ({
      status: 409,
      body: { message: "The design changed." },
    }))
    await page.goto("/storefront")
    await card(page).getByRole("button", { name: st.review }).click()
    await page
      .getByRole("region", { name: st.publishTitle })
      .getByRole("button", { name: st.confirm })
      .click()
    await expect(
      page.getByText(
        "The storefront changed. Select a revision to refresh its preview and review it again."
      )
    ).toBeVisible()
    await expect(page.getByRole("region", { name: st.publishTitle })).toBeHidden()
    expect(api.callsTo("POST", `${root()}/publish`)).toHaveLength(1)
  })
})

test("members without administration access never ask for preview links", async ({
  page,
  api,
}) => {
  designs(api)
  api.on("GET", "/me", () => ({ data: { user: member } }))
  await page.goto("/storefront")
  await expect(
    page.getByText("Only workspace owners and admins can review the storefront.")
  ).toBeVisible()
  expect(api.callsTo("GET", `${root()}/preview-links`)).toHaveLength(0)
})
