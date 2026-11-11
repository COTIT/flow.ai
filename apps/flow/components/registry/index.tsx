"use client"

import type { ComponentType } from "react"

import type { FlowComponentType } from "@genesy/api-client/model"
import { AttentionPanel } from "./attention-panel"
import { Chart } from "./chart"
import { Comparison } from "./comparison"
import { InvoiceTable, OrderList, StockList } from "./lists"
import { MetricCard } from "./metric-card"
import { ReportList } from "./report-list"
import type { RegistryProps } from "./types"

const LineChart = (props: RegistryProps) => <Chart {...props} line />
const BarChart = (props: RegistryProps) => <Chart {...props} line={false} />

/**
 * The approved components a saved scene may name. Keyed by B1's vocabulary, so a type the API
 * accepts but Flow can't draw fails the type check; nothing outside this map is ever rendered.
 */
export const REGISTRY = {
  metric_card: MetricCard,
  line_chart: LineChart,
  bar_chart: BarChart,
  comparison: Comparison,
  order_list: OrderList,
  stock_list: StockList,
  attention_panel: AttentionPanel,
  invoice_table: InvoiceTable,
  report_list: ReportList,
} satisfies Record<FlowComponentType, ComponentType<RegistryProps>>

/** The component for a type, or null for one this build doesn't know (it renders nothing). */
export const componentFor = (
  type: string
): ComponentType<RegistryProps> | null =>
  Object.hasOwn(REGISTRY, type) ? REGISTRY[type as FlowComponentType] : null
