// Combined adversarial persona: a double-clicker on a flaky network with a second tab open.
// SIMULATED API and voice (see harness.ts). Seeds 3, 17, 404, 9001, 77777 green at 1280/834/375.
import { sc } from "../scenes-api"
import { readBack, scene, stockRoutes } from "../task"
import { v } from "../voice"
import { test } from "./harness"

test("combo: double-clicker, flaky network, two tabs", async ({
  persona,
  page,
  api,
  context,
}) => {
  const stock =
    readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on(
    "GET",
    "/workspace/:ws/products/:id/stock-with-bins",
    () => stock.body as object
  )
  await stockRoutes(page)
  await page.goto("/")
  let other: Awaited<ReturnType<typeof persona.secondTab>> | null = null
  const live = () => page.getByRole("button", { name: v.addFiles }).isVisible()
  await persona.run([
    {
      name: "double-click a scene card",
      weight: 3,
      run: async () => {
        await persona.backHome()
        const links = page
          .getByRole("region", { name: sc.yourFlow })
          .getByRole("link")
        const n = await links.count()
        if (n) await persona.click(links.nth(persona.int(0, n - 1)), "double")
      },
    },
    {
      name: "triple-click back home",
      weight: 2,
      run: () =>
        persona.click(
          page.getByRole("button", { name: "Back to home" }),
          "triple"
        ),
    },
    {
      name: "start text (double)",
      weight: 2,
      run: () =>
        persona.click(
          page.getByRole("button", { name: v.textInstead, exact: true }),
          "double"
        ),
    },
    {
      name: "say twice fast",
      weight: 3,
      run: async () => {
        await persona.say("Sales today?", "enter")
        await persona.say("Sales today?", "enter")
      },
    },
    {
      name: "agent proposes stock",
      weight: 2,
      run: async () => {
        if (await live())
          persona.agent.call("stock_set_levels", {
            path: { product: "p-500" },
            body: {
              warehouses: [
                {
                  warehouse_id: "wh-main",
                  stock_lvl: persona.pick([110, 120, 130]),
                },
              ],
            },
          })
      },
    },
    {
      name: "hammer approve",
      weight: 3,
      run: () =>
        persona.click(
          scene(page).getByRole("button", { name: v.approve }),
          persona.pick(["double", "triple"] as const)
        ),
    },
    {
      name: "a scene data call fails",
      weight: 2,
      run: () =>
        persona.failApi(
          new RegExp(
            persona.pick([
              "/sales-summary",
              "/attention",
              "/orders",
              "/sales/daily",
            ])
          ),
          persona.pick([500, 503, 429]),
          4
        ),
    },
    {
      name: "offline blip",
      weight: 1,
      run: async () => {
        await persona.offline(context, true)
        await page.waitForTimeout(persona.int(200, 1500))
        await persona.offline(context, false)
      },
    },
    { name: "reload", weight: 1, run: () => persona.reload() },
    { name: "back", weight: 2, run: () => persona.history("back") },
    {
      name: "open a second tab",
      weight: 1,
      run: async () => {
        if (other) return
        other = await persona.secondTab(api)
        await other.page.goto(
          persona.pick(["/", "/scenes/pulse", "/?view=task"])
        )
      },
    },
    {
      name: "second tab clicks around",
      weight: 2,
      run: async () => {
        if (other) await persona.clickAnything(other.page.locator("main"))
      },
    },
  ])
})
