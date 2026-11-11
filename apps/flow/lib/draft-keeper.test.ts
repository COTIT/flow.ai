import { describe, expect, it } from "vitest"

import { keepDraft, keptDraft } from "./draft-keeper"

const tab = () => {
  const items = new Map<string, string>()
  return {
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
    items,
  }
}

describe("draft keeper", () => {
  it("gives the draft back to the same person in the same workspace only", () => {
    const storage = tab()
    keepDraft(storage, "u1:w1", "Half a thought")
    expect(keptDraft(storage, "u1:w1")).toBe("Half a thought")
    expect(keptDraft(storage, "u2:w1")).toBe("")
    // Another owner's draft is dropped, not kept for later.
    expect(keptDraft(storage, "u1:w1")).toBe("")
  })

  it("an empty box (sent or cleared) or no owner keeps nothing", () => {
    const storage = tab()
    keepDraft(storage, "u1:w1", "Half a thought")
    keepDraft(storage, "u1:w1", "")
    expect(storage.items.size).toBe(0)
    keepDraft(storage, null, "Half a thought")
    expect(storage.items.size).toBe(0)
  })

  it("reads nothing from a malformed or foreign entry, and survives a storage that throws", () => {
    const storage = tab()
    for (const raw of ["not json", "null", "[]", '{"owner":"u1:w1"}', '{"owner":1,"text":"x"}']) {
      storage.items.set("genesy.flow.draft", raw)
      expect(keptDraft(storage, "u1:w1")).toBe("")
    }
    const broken = {
      getItem: () => {
        throw new Error("denied")
      },
      setItem: () => {
        throw new Error("denied")
      },
      removeItem: () => {
        throw new Error("denied")
      },
    }
    expect(keptDraft(broken, "u1:w1")).toBe("")
    expect(() => keepDraft(broken, "u1:w1", "x")).not.toThrow()
  })
})
