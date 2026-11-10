import type { Frame, Page } from "@playwright/test"

import { expectNoHorizontalScroll } from "../../src/test"
import {
  designs,
  draft,
  expect,
  loggedOut,
  member,
  previewFrame,
  revisionButton,
  root,
  test,
} from "./storefront-api"

const frameAncestors = (csp: string | undefined) =>
  csp
    ?.split(";")
    .map((d) => d.trim())
    .find((d) => d.startsWith("frame-ancestors"))

test("only /storefront/preview may be framed, and only by Flow itself", async ({
  request,
}) => {
  const preview = await request.get(`/storefront/preview?revision=${draft}`)
  expect(frameAncestors(preview.headers()["content-security-policy"])).toBe(
    "frame-ancestors 'self'"
  )
  for (const path of [
    "/",
    "/login",
    "/storefront",
    "/storefront/preview/x",
    "/storefront/previewx",
  ]) {
    const response = await request.get(path, { maxRedirects: 0 })
    expect(
      frameAncestors(response.headers()["content-security-policy"]),
      path
    ).toBe("frame-ancestors 'none'")
  }
})

/** Records every /api/voice request the preview frame makes. */
function voiceFromFrame(page: Page) {
  const seen: string[] = []
  page.on("request", (r) => {
    if (
      r.url().includes("/api/voice") &&
      r.frame().url().includes("/storefront/preview")
    )
      seen.push(r.url())
  })
  return seen
}

async function layout(frame: Frame) {
  return frame.evaluate(() => {
    const grid = document.querySelector("ul")!
    const hero = document.querySelector("section")!
    return {
      width: window.innerWidth,
      columns: getComputedStyle(grid).gridTemplateColumns.split(" ").length,
      hero: getComputedStyle(hero).display,
    }
  })
}

async function checkModes(page: Page) {
  const frame = await previewFrame(page)
  await expect(frame.getByText("Honey jar 4")).toBeVisible()
  // Fixture columns: mobile 1, tablet 2, desktop 3; the split hero is a grid from md up.
  for (const [mode, width, columns, hero] of [
    ["Mobile", 375, 1, "flex"],
    ["Tablet", 834, 2, "grid"],
    ["Desktop", 1280, 3, "grid"],
  ] as const) {
    await page.getByRole("button", { name: mode, exact: true }).click()
    await expect(
      page.getByRole("button", { name: mode, exact: true })
    ).toHaveAttribute("aria-pressed", "true")
    await expect.poll(() => layout(frame)).toEqual({ width, columns, hero })
    await expectNoHorizontalScroll(page)
  }
}

test("mobile, tablet and desktop render the real breakpoint layout inside the frame", async ({
  page,
  api,
}) => {
  designs(api)
  const voice = voiceFromFrame(page)
  await page.goto("/storefront")
  await page.getByRole("button", { name: revisionButton(2) }).click()
  await checkModes(page)
  expect(voice).toEqual([])
})

test.describe("on a tablet-sized page", () => {
  test.use({ viewport: { width: 834, height: 1194 }, hasTouch: true })

  test("all three modes still render their own layout", async ({
    page,
    api,
  }) => {
    designs(api)
    await page.goto("/storefront")
    await page.getByRole("button", { name: revisionButton(2) }).click()
    await checkModes(page)
  })
})

test.describe(() => {
  test.use({
    allow: {
      consolePrefix: ["Unsafe attempt to initiate navigation"],
    },
  })

  test("the preview is inert: no links, forms, writes or navigation", async ({
    page,
    api,
  }) => {
    designs(api)
    const voice = voiceFromFrame(page)
    await page.goto("/storefront")
    await page.getByRole("button", { name: revisionButton(2) }).click()
    const frame = await previewFrame(page)
    await expect(frame.getByText("Honey jar 1")).toBeVisible()
    const frameUrl = frame.url()
    const moves: string[] = []
    page.on("framenavigated", (f) => {
      if (f === page.mainFrame()) moves.push(f.url())
    })
    expect(await frame.locator("a, form").count()).toBe(0)
    await frame.getByText("Shop the collection").click()
    await frame.getByText("Honey jar 1").click()
    // The sandbox refuses a script in the frame moving the top window.
    await frame
      .evaluate(() => {
        window.top!.location.href = "/login"
      })
      .catch(() => {})
    await page.waitForTimeout(500)
    expect(moves).toEqual([])
    await expect(page).toHaveURL(/\/storefront$/)
    expect(frame.url()).toBe(frameUrl)
    expect(api.calls.filter((c) => c.method !== "GET")).toHaveLength(0)
    expect(voice).toEqual([])
  })
})

test("opened on its own, the preview goes back to the review page", async ({
  page,
  api,
}) => {
  designs(api)
  await page.goto(`/storefront/preview?revision=${draft}`)
  await expect(page).toHaveURL(/\/storefront$/)
})

/** Puts the preview frame on the current page, as a same-origin page could. */
async function injectFrame(page: Page) {
  await page.evaluate((src) => {
    const frame = document.createElement("iframe")
    frame.src = src
    frame.title = "Injected"
    document.body.append(frame)
  }, `/storefront/preview?revision=${draft}`)
  await expect
    .poll(() =>
      page.frames().some((f) => f.url().includes("/storefront/preview"))
    )
    .toBe(true)
  return page.frames().find((f) => f.url().includes("/storefront/preview"))!
}

test.describe(() => {
  test.use({
    allow: {
      responses: [
        {
          url: `/v1/workspace/w1/storefront/pages/home/preview?revision_id=${draft}`,
          status: 403,
        },
      ],
    },
  })

  test("a member's frame gets the API's refusal and no draft content", async ({
    page,
    api,
  }) => {
    designs(api)
    api.on("GET", "/me", () => ({ data: { user: member } }))
    api.on("GET", `${root()}/preview`, () => ({
      status: 403,
      body: { message: "Forbidden." },
    }))
    await page.goto("/storefront")
    await expect(
      page.getByText(
        "Only workspace owners and admins can review the storefront."
      )
    ).toBeVisible()
    const frame = await injectFrame(page)
    await expect(
      frame.getByText("This preview is not available.")
    ).toBeVisible()
    await expect(frame.getByText("From farm to your table")).toHaveCount(0)
  })
})

test("logged out, the frame asks to log in and calls nothing", async ({
  page,
  api,
}) => {
  designs(api)
  await loggedOut(page)
  await page.goto("/login")
  const frame = await injectFrame(page)
  await expect(
    frame.getByText("Log in again to see the preview.")
  ).toBeVisible()
  expect(api.calls.filter((c) => c.path.includes("/storefront/"))).toHaveLength(
    0
  )
})
