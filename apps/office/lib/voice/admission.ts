import "server-only"
import { VoiceError } from "./server"
import { voiceSetting } from "./settings"

/** Disable new work without disabling status, cancellation or Rust provider cleanup. */
export async function assistantEnabled(): Promise<boolean> {
  const value = await voiceSetting("ASSISTANT_ENABLED")
  if (!["", "true", "false"].includes(value))
    throw new VoiceError("Assistant configuration is invalid.", 503)
  return value === "true"
}

/** Explicit rollout scope; an empty list never admits every workspace. */
export async function assistantWorkspaces(): Promise<string[]> {
  const value = await voiceSetting("ASSISTANT_WORKSPACE_ALLOWLIST")
  const rows = value.split(",").map((row) => row.trim().toLowerCase())
  if (
    rows.length > 100 ||
    rows.some(
      (row) =>
        !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
          row
        )
    )
  )
    throw new VoiceError("Assistant rollout configuration is invalid.", 503)
  return rows
}

export async function requireAssistantAdmission(
  workspaceId?: string
): Promise<void> {
  if (!(await assistantEnabled()))
    throw new VoiceError(
      "The Office assistant is temporarily unavailable.",
      503
    )
  if (
    workspaceId !== undefined &&
    !(await assistantWorkspaces()).includes(workspaceId.toLowerCase())
  )
    throw new VoiceError(
      "The assistant is not enabled for this workspace.",
      403
    )
}
