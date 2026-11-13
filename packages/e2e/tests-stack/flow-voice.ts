import type { Page } from "@playwright/test"

import type { SessionHistory } from "../../voice/src/lib/history"
import { flowVoice } from "../mock/flow-voice.mjs"
import { expect } from "../src/test"

/** Fake transport only; real business reads, reviews and approvals keep their existing routes. */
export async function fakeFlowTransport(page: Page) {
  let owner: { authorization: string; workspaceId: string } | undefined
  await page.route(/\/api\/voice\/(session|history)$/, async (route) => {
    const request = route.request()
    const body = request.postDataJSON() as Record<string, unknown>
    const authorization = request.headers().authorization
    const workspaceId = body.workspaceId
    if (!authorization || typeof workspaceId !== "string")
      throw new Error("Fake Flow transport needs an authenticated workspace.")
    if (
      owner &&
      (authorization !== owner.authorization ||
        workspaceId !== owner.workspaceId)
    )
      throw new Error("Fake Flow transport cannot change login or workspace.")
    owner ??= { authorization, workspaceId }
    if (request.method() === "DELETE")
      return route.fulfill({ json: { status: "ended" } })

    const path = new URL(request.url()).pathname
    const reply = { status: 500, json: {} as Record<string, unknown> }
    const handled = await flowVoice(
      { method: request.method() },
      null,
      path,
      (_res: unknown, status: number, json: Record<string, unknown>) =>
        Object.assign(reply, { status, json }),
      async () => body
    )
    expect(handled).toBe(true)
    if (path.endsWith("/session") && reply.json.history) {
      const me = await page.request.get(new URL("/v1/me", request.url()).href, {
        headers: { Authorization: authorization },
      })
      expect(me.status()).toBe(200)
      const { data } = (await me.json()) as { data: { user: { id: string } } }
      expect(typeof data.user.id).toBe("string")
      Object.assign(reply.json, {
        sessionId: "stack-session",
        signedUrl:
          "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=stack",
        history: {
          ...(reply.json.history as SessionHistory),
          userId: data.user.id,
        },
      })
    }
    return route.fulfill(reply)
  })
}
