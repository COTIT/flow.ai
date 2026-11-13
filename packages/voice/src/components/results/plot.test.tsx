import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"
import { ResultPlot, type PlotKind } from "./plot"

// Inspect the SVG contract passed to Recharts; browser tests cover its mounted drawing.
vi.mock("recharts", () => {
  const Chart = ({
    children,
    ref,
  }: {
    children?: ReactNode
    ref?: (svg: { setAttribute: (name: string, value: string) => void }) => void
  }) => {
    let fontFamily: string | undefined
    ref?.({
      setAttribute: (_, value) => {
        fontFamily = value
      },
    })
    return <svg fontFamily={fontFamily}>{children}</svg>
  }
  const Shape = ({
    fill,
    stroke,
    data,
  }: {
    fill?: string
    stroke?: string
    data?: { fill: string }[]
  }) => <path fill={data?.[0]?.fill ?? fill} stroke={stroke} />
  const Axis = ({ tick }: { tick?: { fill: string; fontFamily?: string } }) => (
    <text fill={tick?.fill} fontFamily={tick?.fontFamily}>
      Label
    </text>
  )
  return {
    PieChart: Chart,
    BarChart: Chart,
    LineChart: Chart,
    Pie: Shape,
    Bar: Shape,
    Line: Shape,
    XAxis: Axis,
    YAxis: Axis,
    CartesianGrid: Shape,
    Tooltip: () => null,
    ResponsiveContainer: Chart,
  }
})

describe("report plot SVG", () => {
  it.each(["line", "sparkline", "bar", "hbar", "pie", "donut"] as PlotKind[])(
    "%s uses explicit print colors and registered font",
    (kind) => {
      const svg = renderToStaticMarkup(
        <ResultPlot
          kind={kind}
          items={[{ label: "Honey", value: 1250, shown: "$12.50" }]}
          label="Sales"
          width={720}
          height={320}
          exportMode
        />
      )
      expect(svg).not.toContain("var(")
      expect(svg).toContain('font-family="Poppins"')
      expect(svg).toContain("#d0a55f")
    }
  )
})
