import assert from "node:assert/strict"
import { test } from "node:test"

import { flowVoice } from "../mock/flow-voice.mjs"
import { PORTS, apiEnv, voiceEnv } from "./stack.mjs"

const base = { credentialsKey: "k", files: { keyId: "GK1", secret: "s" } }

test("voice stays off unless the stack was brought up with VOICE=1", () => {
  assert.deepEqual(voiceEnv(base, ["w1"]), {})
  assert.equal(
    apiEnv(base).ASSISTANT_BRIDGE_SECRET,
    process.env.ASSISTANT_BRIDGE_SECRET
  )
})

test("VOICE=1 enables the assistant for the given workspaces only, with the restricted checkpoint login", () => {
  const state = {
    ...base,
    voice: { bridgeSecret: "b".repeat(64), checkpointPassword: "p1" },
  }
  const env = voiceEnv(state, ["w1"])
  assert.equal(env.ASSISTANT_ENABLED, "true")
  assert.equal(env.ASSISTANT_WORKSPACE_ALLOWLIST, "w1")
  assert.equal(env.ASSISTANT_BRIDGE_SECRET, "b".repeat(64))
  assert.equal(
    env.ASSISTANT_CHECKPOINT_URL,
    `postgres://assistant_checkpoint_local:p1@127.0.0.1:${PORTS.postgres}/genesy`
  )
  assert.equal(apiEnv(state).ASSISTANT_BRIDGE_SECRET, "b".repeat(64))
})

const mockFlow = async (route, body) => {
  let reply
  const handled = await flowVoice(
    { method: "POST" },
    null,
    `/api/voice/${route}`,
    (_res, status, value) => {
      reply = { status, body: value }
    },
    async () => body
  )
  return { handled, ...reply }
}

test("Flow bootstrap issues history when requested and keeps an explicit logical identity", async () => {
  const legacy = await mockFlow("session", {
    workspaceId: "w1",
    textOnly: true,
  })
  assert.equal(legacy.body.history, undefined)
  const first = await mockFlow("session", {
    workspaceId: "w1",
    textOnly: true,
    history: true,
  })
  const fresh = await mockFlow("session", {
    workspaceId: "w1",
    textOnly: true,
    history: true,
  })
  assert.ok(first.body.history?.conversationId)
  assert.notEqual(
    first.body.history.conversationId,
    fresh.body.history.conversationId
  )
  const continued = await mockFlow("session", {
    workspaceId: "w1",
    textOnly: true,
    history: true,
    logicalConversationId: first.body.history.conversationId,
  })
  assert.equal(
    continued.body.history.conversationId,
    first.body.history.conversationId
  )
  assert.ok(continued.body.history.sourceId)
  assert.notEqual(continued.body.history.sourceId, first.body.history.sourceId)
  assert.equal(continued.body.history.userId, "u1")
  assert.equal(continued.body.history.context, "{}")
})

test("Flow history mock acknowledges durable events and serves empty archive reads", async () => {
  const saved = await mockFlow("history", {
    command: "events",
    workspaceId: "w1",
    logicalConversationId: "logical-1",
    sourceId: "source-1",
    events: [],
  })
  assert.deepEqual(saved, {
    handled: true,
    status: 200,
    body: { status: "saved" },
  })
  const list = await mockFlow("history", { command: "list", workspaceId: "w1" })
  assert.deepEqual(list.body.conversations, [])
  const metadata = await mockFlow("history", {
    command: "get",
    workspaceId: "w1",
    logicalConversationId: "logical-1",
  })
  assert.equal(metadata.body.conversation.id, "logical-1")
  const messages = await mockFlow("history", {
    command: "messages",
    workspaceId: "w1",
    logicalConversationId: "logical-1",
    revision: 3,
  })
  assert.deepEqual(messages.body.messages, [])
  assert.equal(messages.body.revision, 3)
})
