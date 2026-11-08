import "server-only"
import {
  farmGrants,
  verifyLogin,
  voiceBearer,
  VoiceError,
  type VoiceContext,
} from "./server"
import { assistantRequest, requireLive, type AssistantSession } from "./bridge"
import { withVoiceTracing } from "./tracing"

/** Rust `GET …/assistant/sessions/{id}?with=identity`: the bearer's own claims, never cached. */
type SessionIdentity = AssistantSession & {
  identity?: {
    workspaces?: unknown
    permissions?: unknown
    farm_roles?: unknown
  }
}

/** One backend read verifies login, workspace, claims and session; older APIs fall back to `/me` checks. */
export async function authorizeVoiceSession(
  request: Request,
  workspaceId: unknown,
  sessionId: unknown,
  mode: "live" | "read"
): Promise<{ context: VoiceContext; session: AssistantSession }> {
  const base = await voiceBearer(request, workspaceId)
  if (typeof sessionId !== "string" || !/^[a-zA-Z0-9-]{1,100}$/.test(sessionId))
    throw new VoiceError("Start an assistant session first.")
  const found = await withVoiceTracing("session_lookup", async () => {
    try {
      return await assistantRequest<SessionIdentity>(
        base,
        `/sessions/${encodeURIComponent(sessionId)}?with=identity`
      )
    } catch (error) {
      if (error instanceof VoiceError && error.status === 401)
        throw new VoiceError("Your session could not be verified.", 401)
      if (error instanceof VoiceError && error.status === 403)
        throw new VoiceError("You do not have access to this workspace.", 403)
      throw error
    }
  })
  const { identity, ...session } = found
  let context: VoiceContext
  if (identity && typeof identity === "object") {
    if (
      !Array.isArray(identity.workspaces) ||
      !identity.workspaces.includes(base.workspaceId) ||
      !Array.isArray(identity.permissions) ||
      !Array.isArray(identity.farm_roles)
    )
      throw new VoiceError("You do not have access to this workspace.", 403)
    context = {
      ...base,
      permissions: [
        ...identity.permissions.filter(
          (permission): permission is string => typeof permission === "string"
        ),
        ...farmGrants(identity.farm_roles),
      ],
    }
  } else context = await withVoiceTracing("authorize", () => verifyLogin(base))
  if (mode === "live") requireLive(session)
  context.sessionId = session.id
  context.language = session.language ?? "en"
  return { context, session }
}
