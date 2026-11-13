import { createHmac } from "node:crypto"
import { describe, expect, it } from "vitest"
import {
  historyContext,
  providerTranscript,
  verifyTranscriptSignature,
} from "./history-transcript"

describe("provider transcript trust boundary", () => {
  const now = 1_800_000_000
  const secret = "synthetic-webhook-secret-for-tests-only"
  const body = new TextEncoder().encode('{"type":"post_call_transcription"}')
  const header = (timestamp = now, bytes = body) =>
    `t=${timestamp},v0=${createHmac("sha256", secret).update(`${timestamp}.`).update(bytes).digest("hex")}`

  it("verifies exact bytes and rejects tampering, stale/future stamps and missing secrets", () => {
    expect(() =>
      verifyTranscriptSignature(body, header(), secret, now)
    ).not.toThrow()
    for (const args of [
      [new TextEncoder().encode("{}"), header(), secret],
      [body, header(now - 1801), secret],
      [body, header(now + 301), secret],
      [body, header(), ""],
      [body, "t=NaN,v0=bad", secret],
    ] as const)
      expect(() =>
        verifyTranscriptSignature(args[0], args[1], args[2], now)
      ).toThrow()
  })

  it("whitelists dialogue, preserves source order and redacts native approvals", () => {
    const result = providerTranscript(
      {
        conversation_id: "conv_test",
        agent_id: "agent_test",
        status: "done",
        metadata: { text_only: false },
        transcript: [
          {
            id: "m1",
            role: "user",
            message: "Use a blue header",
            source_medium: "text",
          },
          {
            role: "agent",
            message: "private scene",
            contextual_update_info: {},
          },
          {
            role: "agent",
            tool_calls: [{ token: "never-store" }],
            message: null,
          },
          { id: "m2", role: "user", message: "Confirm action 123456" },
          {
            id: "m3",
            role: "agent",
            message: "Use red instead",
            interrupted: true,
          },
        ],
        analysis: {
          transcript_summary: "Header decision changed. Confirm batch 654321",
          token: "never-store",
        },
        conversation_initiation_client_data: {
          dynamic_variables: { secret__token: "never-store" },
        },
      },
      "agent_test"
    )
    expect(result.turns).toHaveLength(3)
    expect(result.turns[0]).toMatchObject({
      key: "m1",
      role: "user",
      text: "Use a blue header",
      modality: "text",
    })
    expect(result.turns[1]?.text).not.toContain("123456")
    expect(result.turns[2]?.interrupted).toBe(true)
    expect(JSON.stringify(result)).not.toContain("never-store")
    expect(result.summary).not.toContain("654321")
  })

  it("refuses mismatched identity, omitted transcript and nonterminal snapshots", () => {
    const base = {
      conversation_id: "conv_test",
      agent_id: "agent_test",
      status: "done",
      transcript: [],
    }
    for (const input of [
      { ...base, agent_id: "other" },
      { ...base, transcript: undefined },
      { ...base, status: "processing" },
      { ...base, conversation_id: "../foreign" },
    ])
      expect(() => providerTranscript(input, "agent_test")).toThrow()
  })
})

describe("bounded untrusted history bootstrap", () => {
  it("keeps recent exact dialogue, provenance and gaps; excludes pending drafts", () => {
    const messages = Array.from({ length: 30 }, (_, i) => ({
      id: String(i),
      source_id: "source",
      ordinal: i,
      role: "user",
      text: `Preference ${i}`,
      status: "complete",
      provenance: "final",
      modality: "text",
    }))
    messages.push({
      ...messages[0]!,
      id: "queued",
      ordinal: 31,
      text: "NEVER SENT",
      status: "queued",
    })
    const context = JSON.parse(
      historyContext(
        { id: "conversation", revision: 3, coverage: "partial" },
        [{ id: "source", ordinal: 0, summary: "A decision was discussed." }],
        { messages, through_sequence: "0:29", gaps: ["missing-source"] }
      )
    )
    expect(context.messages).toHaveLength(20)
    expect(context.messages[0].text).toBe("Preference 10")
    expect(context.messages.at(-1).text).toBe("Preference 29")
    expect(context.gaps).toEqual(["missing-source"])
    expect(context.instruction).toMatch(/untrusted/i)
    expect(JSON.stringify(context)).not.toContain("NEVER SENT")
  })

  it("bounds UTF-8 bytes without silently claiming full restoration", () => {
    const text = "漢".repeat(8000)
    const value = historyContext(
      { id: "c", revision: 1, coverage: "complete" },
      Array.from({ length: 10 }, (_, i) => ({
        id: String(i),
        ordinal: i,
        summary: text,
      })),
      {
        messages: Array.from({ length: 25 }, (_, i) => ({
          id: String(i),
          role: "user",
          text,
          status: "complete",
          provenance: "final",
        })),
        gaps: [],
      }
    )
    expect(new TextEncoder().encode(value).byteLength).toBeLessThanOrEqual(
      32768
    )
    expect(JSON.parse(value).omitted).toBe(true)
  })
})
