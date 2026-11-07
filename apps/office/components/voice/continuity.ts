/** Memory-only references from authenticated results; never a substitute for Rust ownership. */
export type TaskReference = {
  kind: "action" | "batch"
  id: string
  sessionId: string
  workspaceId: string
  status: string
  seenAt: number
}
export type BatchProgress = {
  id: string
  summary?: string
  state: string
  nextIndex: number
  recoverable: boolean
  steps: {
    status: string
    message?: string
    data?: { actionId?: string; summary?: string; operation?: string }
  }[]
}
const lifetime = 600000
const statuses = new Set([
  "awaiting_review",
  "proposed",
  "approved",
  "running",
  "queued",
  "partial",
  "completed",
  "cancelled",
  "expired",
  "failed",
  "unknown",
  "paused",
  "not_started",
])
const live = (task: TaskReference, workspaceId: string, now: number) =>
  task.workspaceId === workspaceId &&
  task.seenAt <= now &&
  now - task.seenAt < lifetime

export function rememberTask(
  tasks: TaskReference[],
  task: TaskReference,
  now: number
): TaskReference[] {
  return [
    ...tasks.filter(
      (entry) =>
        live(entry, task.workspaceId, now) &&
        !(entry.kind === task.kind && entry.id === task.id)
    ),
    {
      kind: task.kind,
      id: task.id,
      sessionId: task.sessionId,
      workspaceId: task.workspaceId,
      status: statuses.has(task.status) ? task.status : "unknown",
      seenAt: task.seenAt,
    },
  ].slice(-100)
}

export function taskOwner(
  tasks: TaskReference[],
  kind: TaskReference["kind"],
  id: string,
  workspaceId: string,
  now: number
) {
  return tasks.find(
    (task) =>
      task.kind === kind && task.id === id && live(task, workspaceId, now)
  )?.sessionId
}

/** Fresh provider receives no transcript, user approval, native ticket, credentials or session ID. */
export function historicalProgress(
  tasks: TaskReference[],
  workspaceId: string,
  now: number
) {
  return tasks
    .filter((task) => live(task, workspaceId, now))
    .map(({ kind, id, status }) => ({ kind, id, status }))
}
