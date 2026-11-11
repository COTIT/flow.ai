import type { ResultView } from "./views"
import type { TranscriptRow } from "./helpers"

/** Adds a result picture to the transcript. Sales answers to one question share one chart
 * (display grouping only; nothing is added up). */
export function withView(rows: TranscriptRow[], view: ResultView, id: string) {
  const last = rows.at(-1)
  if (view.kind === "sales" && last?.role === "result" && last.views?.every((v) => v.kind === "sales"))
    return [...rows.slice(0, -1), { ...last, views: [...last.views, view] }]
  return [...rows.slice(-199), { id, role: "result" as const, text: "", views: [view] }]
}
