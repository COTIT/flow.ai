import { describe, expect, it } from "vitest"
import {
  readHistoryPointer,
  saveHistoryPointer,
  HISTORY_POINTER_KEY,
} from "./history-pointer"

const storage = () => {
  const map = new Map<string, string>()
  return {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => void map.set(key, value),
    removeItem: (key: string) => void map.delete(key),
  }
}

describe("history pointer", () => {
  it("stores only opaque id and authenticated user/workspace owner, survives reload", () => {
    const store = storage()
    saveHistoryPointer(store, "user-1:workspace-1", "conversation-1")
    expect(JSON.parse(store.getItem(HISTORY_POINTER_KEY)!)).toEqual({
      owner: "user-1:workspace-1",
      id: "conversation-1",
    })
    expect(readHistoryPointer(store, "user-1:workspace-1")).toBe(
      "conversation-1"
    )
  })
  it("never restores another user or workspace and clears invalid pointers", () => {
    const store = storage()
    saveHistoryPointer(store, "user-1:workspace-1", "conversation-1")
    expect(readHistoryPointer(store, "user-2:workspace-1")).toBeNull()
    expect(store.getItem(HISTORY_POINTER_KEY)).toBeNull()
    store.setItem(HISTORY_POINTER_KEY, "{broken")
    expect(readHistoryPointer(store, "user-1:workspace-1")).toBeNull()
    expect(store.getItem(HISTORY_POINTER_KEY)).toBeNull()
  })
  it("clear and storage denial never block the assistant", () => {
    const store = storage()
    saveHistoryPointer(store, "u:w", "c")
    saveHistoryPointer(store, "u:w", null)
    expect(readHistoryPointer(store, "u:w")).toBeNull()
    const denied = {
      getItem() {
        throw Error()
      },
      setItem() {
        throw Error()
      },
      removeItem() {
        throw Error()
      },
    }
    expect(readHistoryPointer(denied, "u:w")).toBeNull()
    expect(() => saveHistoryPointer(denied, "u:w", "c")).not.toThrow()
  })
})
