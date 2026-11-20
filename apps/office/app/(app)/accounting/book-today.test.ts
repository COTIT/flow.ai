import { beforeEach, describe, expect, it, vi } from "vitest"

// The book day comes from the setup read; the hooks are stubbed so the rule runs without React.
const setup = vi.hoisted(() => ({ day: undefined as string | undefined }))
vi.mock("@verzo/api-client/auth", () => ({
  useSession: (pick: (s: { workspaceId: string }) => unknown) => pick({ workspaceId: "ws" }),
}))
vi.mock("@/lib/can", () => ({ useCan: () => true }))
vi.mock("@verzo/api-client/endpoints/accounting", () => ({
  useAccountingSetupGet: () => ({
    data: setup.day ? { data: { business_date: setup.day } } : undefined,
  }),
}))

import { today } from "../purchases/shared"
import { useBookToday } from "./shared"

describe("useBookToday", () => {
  beforeEach(() => {
    setup.day = undefined
  })

  it("is the browser's day until the setup loads", () => {
    expect(useBookToday()).toBe(today())
  })

  it("is the book's business date once the setup answers", () => {
    setup.day = "2031-01-02"
    expect(useBookToday()).toBe("2031-01-02")
  })
})
