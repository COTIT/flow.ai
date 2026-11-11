import type { ResultView } from "./views"
import type { TranscriptRow } from "./helpers"

/** Adds a result picture to the transcript. Sales answers to one question share one chart
 * (display grouping only; nothing is added up). */
export function withView(rows: TranscriptRow[], view: ResultView, id: string, operation?: string) {
  const last = rows.at(-1)
  if (view.kind === "sales" && last?.role === "result" && last.views?.every((v) => v.kind === "sales"))
    return [...rows.slice(0, -1), { ...last, views: [...last.views, view] }]
  return [...rows.slice(-199), { id, role: "result" as const, text: "", views: [view], ...(operation ? { operation } : {}) }]
}

/** A changed picture (another drawing, period or split) takes the place of the one it replaces;
 * a picture no longer on screen is never removed: the new one is added instead. */
export function replaceView(rows: TranscriptRow[], replaces: string, view: ResultView, operation: string | undefined, id: string) {
  const index = rows.findIndex((row) => row.id === replaces && row.role === "result")
  if (index < 0) return withView(rows, view, id, operation)
  return rows.map((row, i) =>
    i === index ? { id: row.id, role: "result" as const, text: "", views: [view], ...(operation ? { operation } : {}) } : row
  )
}
