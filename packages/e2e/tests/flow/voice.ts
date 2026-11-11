import type { Page } from "@playwright/test"

import voiceEn from "../../../../packages/i18n/messages/voice/en.json" with { type: "json" }
import flowEn from "../../../../apps/flow/messages/en.json" with { type: "json" }
import { FLOW_ORDER, FLOW_SIGNED_URL } from "../../mock/flow-voice.mjs"
import { expect } from "../../src/test"

/**
 * SIMULATED voice for Flow's conversation specs: the voice routes answer from the mock upstream
 * (packages/e2e/mock/flow-voice.mjs) through Flow's real proxy, and the ElevenLabs socket is a
 * Playwright fake. Live ElevenLabs voice is not exercised here.
 */
export const v = voiceEn.voice
export const chat = flowEn.flow.chat
export const OFFICE = "https://office.genesy.test"
export { FLOW_ORDER, FLOW_SIGNED_URL }

type Event = { type?: string; [key: string]: unknown }

/** A fake agent on the ElevenLabs socket: records what the browser sends, says and calls tools. */
export async function fakeAgent(
  page: Page,
  onUser?: (text: string, agent: Agent) => void,
  /** `refuse()` true: a new connection is closed at once (the network is down). */
  { refuse }: { refuse?: () => boolean } = {}
) {
  const agent = {
    sent: [] as Event[],
    connections: 0,
    emit: (() => {}) as (event: unknown) => void,
    close: () => {},
    say(text: string) {
      agent.emit({
        type: "agent_response",
        agent_response_event: { agent_response: text },
      })
    },
    call(operation: string, args: unknown = {}) {
      agent.emit({
        type: "client_tool_call",
        client_tool_call: {
          tool_name: "execute_genesy_tool",
          tool_call_id: `call-${agent.sent.length}-${operation}`,
          parameters: {
            operation_id: operation,
            arguments_json: JSON.stringify(args),
          },
          expects_response: true,
        },
      })
    },
    results: () =>
      agent.sent.filter((e) => e.type === "client_tool_result").length,
  }
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    if (refuse?.()) return void socket.close()
    agent.connections++
    agent.emit = (event) => socket.send(JSON.stringify(event))
    agent.close = () => socket.close()
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw)) as Event
      agent.sent.push(event)
      if (event.type === "conversation_initiation_client_data")
        agent.emit({
          type: "conversation_initiation_metadata",
          conversation_initiation_metadata_event: {
            conversation_id: "flow-conversation",
            agent_output_audio_format: "pcm_16000",
            user_input_audio_format: "pcm_16000",
          },
        })
      if (event.type === "user_message" && onUser)
        onUser(String(event.text ?? ""), agent)
    })
  })
  return agent
}
export type Agent = Awaited<ReturnType<typeof fakeAgent>>

/** Counts the browser's requests to one Flow voice route (they reach the mock through the proxy). */
export function watch(page: Page, route: string) {
  const seen: { method: string; body: unknown }[] = []
  page.on("request", (request) => {
    if (new URL(request.url()).pathname !== `/api/voice/${route}`) return
    const body = (() => {
      try {
        return request.postDataJSON() as unknown
      } catch {
        return request.postData()
      }
    })()
    seen.push({ method: request.method(), body })
  })
  return seen
}

/** Starts a text conversation from the start controls; returns once it is connected. */
export async function startText(page: Page) {
  await page.getByRole("button", { name: v.textInstead, exact: true }).click()
  // The dock's attach button exists only while connected.
  await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
}

/** Sends one typed message. */
export async function send(page: Page, text: string) {
  await page.getByLabel(v.message).fill(text)
  await page.getByRole("button", { name: v.send, exact: true }).click()
}
