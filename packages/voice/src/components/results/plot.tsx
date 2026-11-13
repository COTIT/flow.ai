"use client"

import { useSyncExternalStore } from "react"
import {
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts"

import type { Kind } from "../../lib/views"

/** Values size the drawing only; text always uses the original API value's formatter. */
export type PlotItem = {
  label: string
  value: number | null
  shown: string
  detail?: string
}
export type PlotKind = Extract<
  Kind,
  "line" | "sparkline" | "bar" | "hbar" | "pie" | "donut"
>

const touchQuery = "(hover: none)"
const subscribeTouch = (change: () => void) => {
  const query = matchMedia(touchQuery)
  query.addEventListener("change", change)
  return () => query.removeEventListener("change", change)
}
const isTouch = () => matchMedia(touchQuery).matches
const serverTouch = () => false
const shade = (i: number) => Math.max(1 - i * 0.12, 0.25)

function ValueTooltip({
  active,
  payload,
}: {
  active?: boolean
  payload?: ReadonlyArray<{ payload?: PlotItem }>
}) {
  const item = payload?.[0]?.payload
  if (!active || !item) return null
  return (
    <div
      role="tooltip"
      className="max-w-72 rounded-xl border border-(--hair) bg-popover p-3 text-sm text-popover-foreground shadow-md"
    >
      <p className="font-medium break-words">{item.label}</p>
      <p className="font-semibold tabular-nums">{item.shown}</p>
      {item.detail ? (
        <p className="text-xs text-muted-foreground">{item.detail}</p>
      ) : null}
    </div>
  )
}

/** Shared chat plot. Fixed dimensions produce complete, nonanimated SVG for report export. */
export function ResultPlot({
  items,
  kind,
  label,
  formatValue,
  width,
  height,
  exportMode = false,
}: {
  items: PlotItem[]
  kind: PlotKind
  label: string
  formatValue?: (value: number) => string
  width?: number
  height?: number
  exportMode?: boolean
}) {
  const touch = useSyncExternalStore(subscribeTouch, isTouch, serverTouch)
  const polar = kind === "pie" || kind === "donut"
  const horizontal = kind === "hbar"
  const spark = kind === "sparkline"
  const fixed = width !== undefined && height !== undefined
  const plotHeight =
    height ??
    (horizontal ? Math.max(260, items.length * 44) : spark ? 96 : undefined)
  const data = items.map((item, i) => ({
    ...item,
    value: polar && item.value !== null ? Math.max(item.value, 0) : item.value,
    fill: "var(--gold)",
    fillOpacity: shade(i),
  }))
  const tooltip = exportMode ? null : (
    <Tooltip
      trigger={touch ? "click" : "hover"}
      content={ValueTooltip}
      isAnimationActive={false}
      cursor={polar ? false : { fill: "var(--hair)", stroke: "var(--hair)" }}
    />
  )
  const dimensions = fixed ? { width, height } : {}
  const chart = polar ? (
    <PieChart
      {...dimensions}
      aria-label={label}
      accessibilityLayer
      margin={{ top: 8, right: 8, bottom: 8, left: 8 }}
    >
      <Pie
        data={data}
        dataKey="value"
        nameKey="label"
        cx="50%"
        cy="50%"
        outerRadius="88%"
        innerRadius={kind === "donut" ? "52%" : 0}
        stroke="var(--card)"
        activeShape={{ stroke: "var(--foreground)", strokeWidth: 3 }}
        inactiveShape={{ opacity: 0.55 }}
        isAnimationActive={false}
      />
      {tooltip}
    </PieChart>
  ) : kind === "line" || spark ? (
    <LineChart
      {...dimensions}
      data={data}
      aria-label={label}
      accessibilityLayer
      margin={{ top: 16, right: 12, bottom: spark ? 4 : 12, left: 4 }}
    >
      {spark ? null : <CartesianGrid vertical={false} stroke="var(--hair)" />}
      <XAxis
        dataKey="label"
        hide={spark}
        minTickGap={28}
        tick={{ fill: "var(--muted-foreground)", fontSize: 11 }}
        tickLine={false}
      />
      <YAxis
        hide={spark}
        width={64}
        tickFormatter={formatValue}
        tick={{ fill: "var(--muted-foreground)", fontSize: 11 }}
        tickLine={false}
        axisLine={false}
      />
      <Line
        dataKey="value"
        type="linear"
        stroke="var(--gold)"
        strokeWidth={2}
        dot={data.length === 1}
        activeDot={{ r: 6, stroke: "var(--foreground)", strokeWidth: 2 }}
        connectNulls={false}
        isAnimationActive={false}
      />
      {tooltip}
    </LineChart>
  ) : (
    <BarChart
      {...dimensions}
      data={data}
      aria-label={label}
      accessibilityLayer
      layout={horizontal ? "vertical" : "horizontal"}
      margin={{ top: 12, right: 12, bottom: 12, left: 4 }}
    >
      <CartesianGrid
        horizontal={!horizontal}
        vertical={horizontal}
        stroke="var(--hair)"
      />
      <XAxis
        type={horizontal ? "number" : "category"}
        dataKey={horizontal ? undefined : "label"}
        tickFormatter={horizontal ? formatValue : undefined}
        minTickGap={28}
        tick={{ fill: "var(--muted-foreground)", fontSize: 11 }}
        tickLine={false}
      />
      <YAxis
        type={horizontal ? "category" : "number"}
        dataKey={horizontal ? "label" : undefined}
        width={horizontal ? 112 : 64}
        tickFormatter={horizontal ? undefined : formatValue}
        tick={{ fill: "var(--muted-foreground)", fontSize: 11 }}
        tickLine={false}
        axisLine={false}
      />
      <Bar
        dataKey="value"
        fill="var(--gold)"
        activeBar={{ stroke: "var(--foreground)", strokeWidth: 2 }}
        maxBarSize={48}
        isAnimationActive={false}
      />
      {tooltip}
    </BarChart>
  )
  return (
    <div
      data-result-chart=""
      aria-label={label}
      className={`w-full min-w-0 ${plotHeight === undefined ? "h-[260px] sm:h-80" : ""}`}
      style={{
        ...(width !== undefined ? { width } : {}),
        ...(plotHeight !== undefined ? { height: plotHeight } : {}),
      }}
    >
      {fixed ? (
        chart
      ) : (
        <ResponsiveContainer
          width="100%"
          height="100%"
          minWidth={0}
          initialDimension={{ width: 640, height: plotHeight ?? 320 }}
        >
          {chart}
        </ResponsiveContainer>
      )}
    </div>
  )
}
