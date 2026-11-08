import assert from "node:assert/strict"
import { test } from "node:test"

import { PORTS, apiEnv, voiceEnv } from "./stack.mjs"

const base = { credentialsKey: "k", files: { keyId: "GK1", secret: "s" } }

test("voice stays off unless the stack was brought up with VOICE=1", () => {
  assert.deepEqual(voiceEnv(base, ["w1"]), {})
  assert.equal(apiEnv(base).ASSISTANT_BRIDGE_SECRET, process.env.ASSISTANT_BRIDGE_SECRET)
})

test("VOICE=1 enables the assistant for the given workspaces only, with the restricted checkpoint login", () => {
  const state = { ...base, voice: { bridgeSecret: "b".repeat(64), checkpointPassword: "p1" } }
  const env = voiceEnv(state, ["w1"])
  assert.equal(env.ASSISTANT_ENABLED, "true")
  assert.equal(env.ASSISTANT_WORKSPACE_ALLOWLIST, "w1")
  assert.equal(env.ASSISTANT_BRIDGE_SECRET, "b".repeat(64))
  assert.equal(env.ASSISTANT_CHECKPOINT_URL, `postgres://assistant_checkpoint_local:p1@127.0.0.1:${PORTS.postgres}/genesy`)
  assert.equal(apiEnv(state).ASSISTANT_BRIDGE_SECRET, "b".repeat(64))
})
