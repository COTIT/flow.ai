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
const wrapLabel = (text: string, columns: number) => {
  const chars = Array.from(text)
  const lines: string[] = []
  for (let i = 0; i < chars.length; i += columns)
    lines.push(chars.slice(i, i + columns).join(""))
  return lines.length ? lines : [""]
}

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
  const legendX = (width ?? 720) * 0.52
  const legendColumns = Math.max(
    1,
    Math.floor(((width ?? 720) - legendX - 36) / 12)
  )
  let legendBottom = 24
  const legend =
    exportMode && polar
      ? items.map((item) => {
          const lines = wrapLabel(item.label, legendColumns)
          const amounts = wrapLabel(item.shown, legendColumns)
          const top = legendBottom
          legendBottom += (lines.length + amounts.length) * 16 + 14
          return { lines, amounts, top }
        })
      : []
  const plotHeight =
    height !== undefined
      ? Math.max(height, legend.length ? legendBottom : 0)
      : horizontal
        ? Math.max(260, items.length * 44)
        : spark
          ? 96
          : undefined
  const pieRadius = Math.min((width ?? 720) * 0.21, (height ?? 320) * 0.44)
  // SVG-to-PDF accepts concrete colors, not theme variables or inherited page fonts.
  const colors = exportMode
    ? {
        gold: "#C5A15B",
        hair: "#E2E3DC",
        muted: "#6D7067",
        foreground: "#252622",
        card: "#ffffff",
      }
    : {
        gold: "var(--gold)",
        hair: "var(--hair)",
        muted: "var(--muted-foreground)",
        foreground: "var(--foreground)",
        card: "var(--card)",
      }
  const fontFamily = exportMode ? "Poppins" : undefined
  const tick = { fill: colors.muted, fontSize: 11, fontFamily }
  const data = items.map((item, i) => ({
    ...item,
    value: polar && item.value !== null ? Math.max(item.value, 0) : item.value,
    fill: colors.gold,
    fillOpacity: shade(i),
  }))
  const tooltip = exportMode ? null : (
    <Tooltip
      trigger={touch ? "click" : "hover"}
      content={ValueTooltip}
      isAnimationActive={false}
      cursor={polar ? false : { fill: colors.hair, stroke: colors.hair }}
    />
  )
  const dimensions = fixed
    ? {
        width,
        height: plotHeight ?? height,
        ref: (svg: SVGSVGElement | null) => {
          if (exportMode && svg) svg.setAttribute("font-family", "Poppins")
        },
      }
    : {}
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
        cx={exportMode ? (width ?? 720) * 0.24 : "50%"}
        cy={exportMode ? (height ?? 320) / 2 : "50%"}
        outerRadius={exportMode ? pieRadius : "88%"}
        innerRadius={
          kind === "donut" ? (exportMode ? pieRadius * 0.59 : "52%") : 0
        }
        stroke={colors.card}
        activeShape={{ stroke: colors.foreground, strokeWidth: 3 }}
        inactiveShape={{ opacity: 0.55 }}
        isAnimationActive={false}
      />
      {tooltip}
      {legend.length ? (
        <g
          data-report-legend=""
          fontFamily="Poppins"
          fontSize={12}
          fill={colors.foreground}
        >
          {legend.map((row, i) => (
            <g key={i}>
              <circle
                cx={legendX + 4}
                cy={row.top + 6}
                r={4}
                fill={colors.gold}
                fillOpacity={shade(i)}
              />
              <text x={legendX + 16} y={row.top + 10}>
                {row.lines.map((line, j) => (
                  <tspan key={j} x={legendX + 16} dy={j ? 16 : 0}>
                    {line}
                  </tspan>
                ))}
                {row.amounts.map((line, j) => (
                  <tspan
                    key={`amount-${j}`}
                    x={legendX + 16}
                    dy={16}
                    fontWeight="bold"
                  >
                    {line}
                  </tspan>
                ))}
              </text>
            </g>
          ))}
        </g>
      ) : null}
    </PieChart>
  ) : kind === "line" || spark ? (
    <LineChart
      {...dimensions}
      data={data}
      aria-label={label}
      accessibilityLayer
      margin={{ top: 16, right: 12, bottom: spark ? 4 : 12, left: 4 }}
    >
      {spark ? null : <CartesianGrid vertical={false} stroke={colors.hair} />}
      <XAxis
        dataKey="label"
        hide={spark}
        minTickGap={28}
        tick={tick}
        tickLine={false}
      />
      <YAxis
        hide={spark}
        width={64}
        tickFormatter={formatValue}
        tick={tick}
        tickLine={false}
        axisLine={false}
      />
      <Line
        dataKey="value"
        type="linear"
        stroke={colors.gold}
        strokeWidth={2}
        dot={data.length === 1}
        activeDot={{ r: 6, stroke: colors.foreground, strokeWidth: 2 }}
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
        stroke={colors.hair}
      />
      <XAxis
        type={horizontal ? "number" : "category"}
        dataKey={horizontal ? undefined : "label"}
        tickFormatter={horizontal ? formatValue : undefined}
        minTickGap={28}
        tick={tick}
        tickLine={false}
      />
      <YAxis
        type={horizontal ? "category" : "number"}
        dataKey={horizontal ? "label" : undefined}
        width={horizontal ? 112 : 64}
        tickFormatter={horizontal ? undefined : formatValue}
        tick={tick}
        tickLine={false}
        axisLine={false}
      />
      <Bar
        dataKey="value"
        fill={colors.gold}
        activeBar={{ stroke: colors.foreground, strokeWidth: 2 }}
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
