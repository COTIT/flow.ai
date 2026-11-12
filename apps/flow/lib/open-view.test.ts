import { describe, expect, it } from "vitest"

import { resolveView, type View } from "./open-view"

const views: View[] = [
  { view: "home", title: "Home", path: "/" },
  { view: "pulse", title: "Morning Pulse", path: "/scenes/pulse" },
  { view: "stock", title: "Stock Watch", path: "/scenes/stock" },
  {
    view: "sales_last_month",
    title: "Sales · last month",
    path: "/scenes/sales_last_month",
  },
  { view: "storefront", title: "Storefront", path: "/storefront" },
  { view: "explore", title: "What can Genesy do?", path: "/explore" },
]

describe("resolveView (P4)", () => {
  it("finds a view by its key", () => {
    expect(resolveView("stock", views)?.path).toBe("/scenes/stock")
    expect(resolveView("sales_last_month", views)?.path).toBe(
      "/scenes/sales_last_month"
    )
  })
  it("finds a view by its title, ignoring case, accents, punctuation and my/the", () => {
    expect(resolveView("Morning Pulse", views)?.view).toBe("pulse")
    expect(resolveView("  my morning-pulse! ", views)?.view).toBe("pulse")
    expect(resolveView("the Stock Wätch", views)?.view).toBe("stock")
    expect(resolveView("what can genesy do", views)?.view).toBe("explore")
    expect(resolveView("Sales last month", views)?.view).toBe(
      "sales_last_month"
    )
  })
  it("never resolves anything outside the list (paths, other origins, unknown names)", () => {
    for (const bad of [
      "",
      "../admin",
      "//evil.test",
      "https://evil.test/scenes/stock",
      "/scenes/stock",
      "scenes/x",
      "settings",
    ])
      expect(resolveView(bad, views), bad).toBeNull()
  })
})
