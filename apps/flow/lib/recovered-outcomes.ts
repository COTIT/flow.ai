import type { RecoveryTask } from "@genesy/voice/lib/recovery"

/** Nothing was written for these: no line to report. Waiting reviews get "Continue review". */
const NOT_RUN = new Set(["awaiting_review", "cancelled", "expired"])

/** Recovered tasks with an outcome this owner has not seen yet (newest list first, max three). */
export function unseenOutcomes(tasks: RecoveryTask[], seen: string[]) {
  const listed = new Set(seen)
  return tasks
    .flatMap((task) => {
      const id = task.data?.batchId ?? task.data?.actionId
      // A batch's steps share its id: one line per batch.
      if (!id || NOT_RUN.has(task.status) || listed.has(id)) return []
      listed.add(id)
      return [{ id, task }]
    })
    .slice(0, 3)
}

/** Only "completed" ran and only "failed" did not; anything else must be checked before repeating. */
export const outcomeKind = (status: string) =>
  status === "completed" ? "ran" : status === "failed" ? "failed" : "unconfirmed"
