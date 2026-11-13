import type { ResultView } from "./views"
import type { TranscriptRow } from "./helpers"

/** Each completed read gets an independent snapshot, including another drawing of older data. */
export function withView(
  rows: TranscriptRow[], view: ResultView, id: string, operation?: string,
  metadata?: Pick<TranscriptRow, "requestId" | "createdAt">,
) {
  return [...rows.slice(-199), { id, role: "result" as const, text: "", views: [view], ...(operation ? { operation } : {}), ...metadata }]
}

/** Legacy `replaces` identifies the source picture; historical bubbles are never rewritten. */
export function replaceView(
  rows: TranscriptRow[], _replaces: string, view: ResultView, operation: string | undefined, id: string,
  metadata?: Pick<TranscriptRow, "requestId" | "createdAt">,
) {
  return withView(rows, view, id, operation, metadata)
}
