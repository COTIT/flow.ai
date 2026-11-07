import { createHash } from "node:crypto"
import {
  authorizeVoice,
  VoiceError,
  voiceFailure,
  voiceResponse,
} from "@/lib/voice/server"
import {
  assistantRequest,
  readBounded,
  requireAssistantSession,
} from "@/lib/voice/bridge"
import { attachmentType, ATTACHMENT_LIMIT } from "@/lib/voice/attachments"
import { providerRequest } from "@/lib/voice/provider"
import { takeFilePermit, voiceInput } from "@/lib/voice/input"
import { requireAssistantAdmission } from "@/lib/voice/admission"
export const runtime = "nodejs"

/** Conversation-only file; ordinary permanent workspace storage is never used implicitly. */
export async function POST(request: Request) {
  let releaseFile: (() => void) | undefined
  let release: (() => Promise<unknown>) | undefined
  let providerFile: { conversation: string; id: string } | undefined
  try {
    await requireAssistantAdmission()
    releaseFile = takeFilePermit(request, true)
    const context = await authorizeVoice(
      request,
      request.headers.get("X-Workspace-Id")
    )
    await requireAssistantAdmission(context.workspaceId)
    const session = await requireAssistantSession(
      context,
      request.headers.get("X-Voice-Session")
    )
    if (!session.conversation_id)
      throw new VoiceError("Conversation identity is not available.", 409)
    const declared = (request.headers.get("Content-Type") ?? "")
      .split(";")[0]!
      .toLowerCase()
    if (
      !["image/jpeg", "image/png", "image/webp", "application/pdf"].includes(
        declared
      )
    )
      throw new VoiceError("Use JPEG, PNG, WebP or PDF.", 415)
    const length = Number(request.headers.get("Content-Length"))
    if (length > ATTACHMENT_LIMIT)
      throw new VoiceError("Files must not exceed 10 MiB.", 413)
    const upload = await assistantRequest<{ id: string }>(
      context,
      "/sessions/" + session.id + "/uploads",
      "POST",
      {
        media_type: declared,
        size_bytes: length > 0 ? length : ATTACHMENT_LIMIT,
      }
    )
    release = () =>
      assistantRequest(
        context,
        "/sessions/" + session.id + "/uploads/" + upload.id,
        "DELETE"
      )
    const bytes = await readBounded(request.body, ATTACHMENT_LIMIT)
    if (!bytes.length || (length > 0 && length !== bytes.length))
      throw new VoiceError("The uploaded file size changed.")
    const type = attachmentType(bytes, declared)
    const form = new FormData()
    form.append(
      "file",
      new Blob([bytes as Uint8Array<ArrayBuffer>], { type }),
      "reference." +
        ({
          "image/jpeg": "jpg",
          "image/png": "png",
          "image/webp": "webp",
          "application/pdf": "pdf",
        }[type] ?? "bin")
    )
    const result = await providerRequest<{ file_id?: string }>(
      "/v1/convai/conversations/" +
        encodeURIComponent(session.conversation_id) +
        "/files",
      { method: "POST", body: form }
    )
    if (!result.file_id || !/^[a-zA-Z0-9_-]{1,200}$/.test(result.file_id))
      throw new VoiceError(
        "Provider returned an invalid attachment reference.",
        503
      )
    providerFile = { conversation: session.conversation_id, id: result.file_id }
    await assistantRequest(
      context,
      "/sessions/" + session.id + "/uploads/" + upload.id + "/complete",
      "POST",
      {
        file_id: result.file_id,
        size_bytes: bytes.length,
        media_type: type,
        sha256: createHash("sha256").update(bytes).digest("hex"),
      },
      true
    )
    return voiceResponse({
      id: upload.id,
      fileId: result.file_id,
      status: "uploaded",
    })
  } catch (error) {
    if (providerFile)
      await providerRequest(
        "/v1/convai/conversations/" +
          encodeURIComponent(providerFile.conversation) +
          "/files/" +
          encodeURIComponent(providerFile.id),
        { method: "DELETE" }
      ).catch(() => undefined)
    await release?.().catch(() => undefined)
    return voiceFailure(error)
  } finally {
    releaseFile?.()
  }
}

export async function DELETE(request: Request) {
  try {
    const { input } = await voiceInput(request)
    const context = await authorizeVoice(request, input.workspaceId)
    const session = await requireAssistantSession(context, input.sessionId)
    if (typeof input.id !== "string" || !/^[0-9a-f-]{36}$/i.test(input.id))
      throw new VoiceError("Invalid attachment handle.")
    const upload = await assistantRequest<{ file_id?: string }>(
      context,
      "/sessions/" + session.id + "/uploads/" + input.id
    )
    if (upload.file_id && session.conversation_id) {
      try {
        await providerRequest(
          "/v1/convai/conversations/" +
            encodeURIComponent(session.conversation_id) +
            "/files/" +
            encodeURIComponent(upload.file_id),
          { method: "DELETE" }
        )
      } catch (error) {
        if (error instanceof VoiceError && error.status === 422)
          throw new VoiceError(
            "This reference has already been shared. End and clear the conversation to remove its context.",
            409
          )
        throw error
      }
    }
    await assistantRequest(
      context,
      "/sessions/" + session.id + "/uploads/" + input.id,
      "DELETE"
    )
    return voiceResponse({ status: "removed" })
  } catch (error) {
    return voiceFailure(error)
  }
}
