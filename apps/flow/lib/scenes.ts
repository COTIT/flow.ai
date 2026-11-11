import type {
  FlowComponent,
  FlowPeriod,
  FlowScene,
  FlowSceneConfig,
} from "@genesy/api-client/model"

/** Built-in scenes (B1's keys); custom `sales_*` / `stock_*` reuse the Sales / Stock layout. */
export const BUILT_IN = [
  "pulse",
  "stock",
  "noticed",
  "orders",
  "sales",
  "routines",
  "studio",
] as const
export type Layout = (typeof BUILT_IN)[number]

export function sceneLayout(key: string): Layout | null {
  if ((BUILT_IN as readonly string[]).includes(key)) return key as Layout
  if (/^sales_[a-z0-9_]{1,26}$/.test(key)) return "sales"
  if (/^stock_[a-z0-9_]{1,26}$/.test(key)) return "stock"
  return null
}

const allRestricted = (s: FlowScene) =>
  s.config.components.length > 0 &&
  s.config.components.every((c) => s.restricted.includes(c.id))

/** Your Flow on home: visible, in order; Flow noticed has its own card; a scene the role can't read at all stays off. */
export const homeScenes = (list: FlowScene[]) =>
  [...list]
    .filter((s) => !s.hidden && s.scene !== "noticed" && !allRestricted(s))
    .sort((a, b) => a.position - b.position)

export const hiddenScenes = (list: FlowScene[]) => list.filter((s) => s.hidden)

const isOrders = (c: FlowComponent) =>
  c.type === "order_list" || !!c.metric?.startsWith("orders.")

/** "Always show pending orders first": order components on top, each group keeps its order. */
export const pendingFirst = (config: FlowSceneConfig): FlowSceneConfig => ({
  components: [
    ...config.components.filter(isOrders),
    ...config.components.filter((c) => !isOrders(c)),
  ],
})

export const keepKey = (period: FlowPeriod) => `sales_${period}`
