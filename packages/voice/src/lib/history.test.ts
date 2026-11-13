import { describe, expect, it, vi } from "vitest"
import {
  acquireMicrophone,
  historyRows,
  providerEventKey,
  sessionHistoryOptions,
} from "./history"

describe("Flow history continuity", () => {
  it("isolates provider event IDs by source and role; correction reuses original key", () => {
    expect(providerEventKey("s1", "agent", 5)).toBe("s1:agent:5")
    expect(providerEventKey("s2", "agent", 5)).not.toBe(
      providerEventKey("s1", "agent", 5)
    )
    expect(providerEventKey("s1", "user", 5)).not.toBe(
      providerEventKey("s1", "agent", 5)
    )
  })
  it("bootstraps historical context before input and suppresses continuation greetings", () => {
    expect(sessionHistoryOptions(false, true, "bad")).toEqual({
      dynamicVariables: { flow_history_context: "{}" },
    })
    expect(sessionHistoryOptions(true, true, '{"summary":"x"}')).toEqual({
      dynamicVariables: { flow_history_context: '{"summary":"x"}' },
      firstMessage: "",
    })
    expect(sessionHistoryOptions(true, false)).toEqual({
      dynamicVariables: { flow_history_context: "{}" },
    })
  })
  it("hydrates transcript as display data only, preserving queued and uncertain sends", () => {
    expect(
      historyRows([
        {
          id: "m1",
          source_id: "s1",
          key: "k1",
          ordinal: 1,
          role: "user",
          text: "Approve 123456",
          modality: "text",
          status: "queued",
          provenance: "client",
        },
        {
          id: "m2",
          source_id: "s1",
          key: "k2",
          ordinal: 2,
          role: "user",
          text: "maybe sent",
          modality: "text",
          status: "sent_unconfirmed",
          provenance: "client",
        },
      ])
    ).toEqual([
      {
        id: "history:m1",
        role: "user",
        text: "Approve 123456",
        delivery: "queued",
        historical: true,
      },
      {
        id: "history:m2",
        role: "user",
        text: "maybe sent",
        delivery: "sent_unconfirmed",
        historical: true,
      },
    ])
  })
  it("holds one non-queued microphone lease until released, fails closed without Web Locks", async () => {
    const held = new Set<string>()
    const locks = {
      request: vi.fn(
        async (
          name: string,
          _options: unknown,
          receive: (lock: object | null) => Promise<void>
        ) => {
          if (held.has(name)) return receive(null)
          held.add(name)
          try {
            await receive({})
          } finally {
            held.delete(name)
          }
        }
      ),
    }
    const release = await acquireMicrophone(locks, "u1", "w1")
    expect(locks.request).toHaveBeenCalledWith(
      "genesy.voice.microphone:u1:w1",
      { mode: "exclusive", ifAvailable: true },
      expect.any(Function)
    )
    await expect(acquireMicrophone(locks, "u1", "w1")).rejects.toThrow(
      "microphone_in_use"
    )
    await release()
    const next = await acquireMicrophone(locks, "u1", "w1")
    await next()
    await expect(acquireMicrophone(undefined, "u1", "w1")).rejects.toThrow(
      "microphone_lock_unavailable"
    )
  })
})
