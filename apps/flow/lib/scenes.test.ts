import { describe, expect, it } from "vitest"
import type { FlowScene } from "@genesy/api-client/model"

import {
  hiddenScenes,
  homeScenes,
  keepKey,
  pendingFirst,
  sceneLayout,
} from "./scenes"

const scene = (key: string, over: Partial<FlowScene> = {}): FlowScene => ({
  scene: key,
  position: 0,
  hidden: false,
  saved: false,
  schema_version: 1,
  restricted: [],
  config: { components: [{ id: "a", type: "attention_panel" }] },
  ...over,
})

describe("homeScenes", () => {
  it("keeps visible scenes in position order without Flow noticed (it has its own card)", () => {
    const list = [
      scene("stock", { position: 1 }),
      scene("noticed", { position: 2 }),
      scene("pulse", { position: 0 }),
      scene("sales", { position: 3, hidden: true }),
    ]
    expect(homeScenes(list).map((s) => s.scene)).toEqual(["pulse", "stock"])
  })
  it("leaves out a scene whose every component is restricted, keeps a partly restricted one", () => {
    const two = {
      components: [
        { id: "a", type: "metric_card" as const },
        { id: "b", type: "report_list" as const },
      ],
    }
    const list = [
      scene("sales", { config: two, restricted: ["a", "b"] }),
      scene("pulse", { config: two, restricted: ["a"] }),
    ]
    expect(homeScenes(list).map((s) => s.scene)).toEqual(["pulse"])
  })
  it("keeps studio (no components) on home", () => {
    expect(
      homeScenes([scene("studio", { config: { components: [] } })]).map(
        (s) => s.scene
      )
    ).toEqual(["studio"])
  })
})

describe("hiddenScenes", () => {
  it("lists hidden ones for Restore", () => {
    expect(
      hiddenScenes([scene("pulse"), scene("sales", { hidden: true })]).map(
        (s) => s.scene
      )
    ).toEqual(["sales"])
  })
})

describe("pendingFirst", () => {
  const config = {
    components: [
      {
        id: "sales-today",
        type: "metric_card" as const,
        metric: "sales.gross" as const,
        period: "today" as const,
      },
      {
        id: "orders-pending",
        type: "metric_card" as const,
        metric: "orders.pending" as const,
        period: "today" as const,
      },
      {
        id: "trend",
        type: "line_chart" as const,
        metric: "sales.gross" as const,
        period: "last_7_days" as const,
      },
      { id: "orders", type: "order_list" as const },
    ],
  }
  it("moves order components to the top, keeping each group's order", () => {
    expect(pendingFirst(config).components.map((c) => c.id)).toEqual([
      "orders-pending",
      "orders",
      "sales-today",
      "trend",
    ])
  })
  it("is stable when already first", () => {
    const once = pendingFirst(config)
    expect(pendingFirst(once)).toEqual(once)
  })
})

describe("keepKey / sceneLayout", () => {
  it("names a kept sales view by its period", () => {
    expect(keepKey("this_month")).toBe("sales_this_month")
  })
  it("maps custom keys to their built-in layout and refuses unknown keys", () => {
    expect(sceneLayout("sales_this_month")).toBe("sales")
    expect(sceneLayout("stock_low")).toBe("stock")
    expect(sceneLayout("pulse")).toBe("pulse")
    expect(sceneLayout("nope")).toBeNull()
  })
})

describe("registry", () => {
  it("draws every component type the scenes API accepts, and nothing else", async () => {
    const { FlowComponentType } = await import("@genesy/api-client/model")
    const { REGISTRY, componentFor } = await import("@/components/registry")
    expect(Object.keys(REGISTRY).sort()).toEqual(
      Object.values(FlowComponentType).sort()
    )
    for (const type of Object.values(FlowComponentType))
      expect(componentFor(type)).not.toBeNull()
    expect(componentFor("pie_chart")).toBeNull()
    expect(componentFor("toString")).toBeNull()
    // Importing the whole registry (UI, api-client) is slow on a cold, busy machine.
  }, 30_000)
})
