// F2 on the real stack: Your Flow layout is the member's own, survives a new session, resets,
// and another workspace's scenes are refused.
import scenesEn from "../../../apps/flow/messages/en/scenes.json" with { type: "json" }
import { expect, loginFlow, test, urls } from "../src/stack"
import { freshOwner } from "./flow-scenes"

const sc = scenesEn.scenes

test("pending orders first is saved by the API, restored after a new login, and reset", async ({
  page,
  request,
  browser,
}) => {
  test.setTimeout(120_000)
  const { login, api } = await freshOwner(request, "scenes")
  await loginFlow(page, login)
  await page.goto(`${urls.flow}/scenes/pulse`)
  const cards = page.locator("[data-component]")
  await expect(cards.first()).toHaveAttribute("data-component", "sales-today")
  // The scene's own actions live in its More menu (keep Flow simple).
  await page.getByRole("button", { name: sc.moreActions }).click()
  await page.getByRole("menuitem", { name: sc.pendingFirst }).click()
  await expect(cards.first()).toHaveAttribute(
    "data-component",
    "orders-pending"
  )

  const saved = await api.get<
    {
      scene: string
      saved: boolean
      config: { components: { id: string }[] }
    }[]
  >(api.ws("/flow/scenes"))
  const pulse = saved.find((s) => s.scene === "pulse")!
  expect(pulse.saved).toBe(true)
  expect(pulse.config.components[0]!.id).toBe("orders-pending")

  // Next session: a new browser context, a new login; the layout comes back from the API.
  const next = await (
    await browser.newContext({ baseURL: urls.flow })
  ).newPage()
  await loginFlow(next, login)
  await next.goto(`${urls.flow}/scenes/pulse`)
  await expect(next.locator("[data-component]").first()).toHaveAttribute(
    "data-component",
    "orders-pending"
  )

  await next.goto(`${urls.flow}/`)
  await next.getByRole("button", { name: sc.reset }).click()
  await next
    .getByRole("alertdialog")
    .getByRole("button", { name: sc.reset })
    .click()
  await expect(
    next.getByRole("status").filter({ hasText: sc.resetDone })
  ).toBeVisible()
  const after = await api.get<{ scene: string; saved: boolean }[]>(
    api.ws("/flow/scenes")
  )
  expect(after.every((s) => !s.saved)).toBe(true)
})

test("another workspace's scenes are refused", async ({ request }) => {
  const a = await freshOwner(request, "scenes-a")
  const b = await freshOwner(request, "scenes-b")
  const res = await request.get(`${urls.api}/v1${b.api.ws("/flow/scenes")}`, {
    headers: { Authorization: `Bearer ${a.api.token}` },
  })
  expect(res.status()).toBe(404)
})
