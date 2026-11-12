// Persona 1, the power user: hops scenes every second, works from the keyboard, starts the next
// thing before the last one finished. SIMULATED API and voice (see harness.ts).
import { sc } from "../scenes-api"
import { readBack, scene, stockRoutes } from "../task"
import { labels, test } from "./harness"

test("power user: fast scene hopping, keyboard, overlapping tasks", async ({
  persona,
  page,
  api,
}) => {
  // The receipt reads the product back after an approved stock change.
  const stock =
    readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on(
    "GET",
    "/workspace/:ws/products/:id/stock-with-bins",
    () => stock.body as object
  )
  await stockRoutes(page)
  await page.goto("/")

  const live = () =>
    page.getByRole("button", { name: labels.addFiles }).isVisible()
  await persona.run([
    {
      name: "open a scene from Your Flow",
      weight: 4,
      run: async () => {
        await persona.backHome()
        const links = page
          .getByRole("region", { name: sc.yourFlow })
          .getByRole("link")
        const n = await links.count()
        if (n) await persona.click(links.nth(persona.int(0, n - 1)))
      },
    },
    { name: "back to home", weight: 2, run: () => persona.backHome() },
    {
      name: "storefront",
      weight: 1,
      run: () =>
        persona.click(
          page.getByRole("link", { name: "Storefront", exact: true })
        ),
    },
    { name: "browser back", weight: 2, run: () => persona.history("back") },
    {
      name: "browser forward",
      weight: 1,
      run: () => persona.history("forward"),
    },
    {
      name: "start typing a conversation",
      weight: 2,
      run: () => persona.startText(),
    },
    {
      name: "ask before the last answer came",
      weight: 4,
      run: () =>
        persona.say(
          persona.pick([
            "Sales today?",
            "Which orders wait?",
            "Stock below minimum",
            "Thanks",
          ])
        ),
    },
    {
      name: "a stock change to review (agent proposes)",
      weight: 2,
      run: async () => {
        if (!(await live())) return
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
      name: "approve whatever waits",
      weight: 3,
      run: () =>
        persona.click(
          scene(page).getByRole("button", { name: labels.approve })
        ),
    },
    {
      name: "tab through",
      weight: 2,
      run: () => persona.tab(persona.int(1, 6), persona.chance(0.3)),
    },
    { name: "escape", weight: 1, run: () => persona.keys("Escape") },
  ])
})
