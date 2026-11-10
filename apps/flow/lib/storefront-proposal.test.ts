import { describe, expect, it } from "vitest"

import {
  PROPOSAL,
  PROPOSAL_READY,
  proposalFromParent,
  readyFromFrame,
} from "./storefront-proposal"

const origin = "https://flow.test"
const frame = {} as Window
const parent = {} as Window
const other = {} as Window
const doc = { theme: "forest", sections: [] }
const collections = [{ section_id: "s1", products: [] }]
const yes = () => true
const no = () => false

const event = (source: unknown, from: string, data: unknown) =>
  ({ source, origin: from, data }) as MessageEvent

describe("the review page hears only its own frame say ready", () => {
  it("accepts the exact message from its frame on this origin", () => {
    expect(readyFromFrame(event(frame, origin, { type: PROPOSAL_READY }), frame, origin)).toBe(true)
  })

  it("ignores another window, another origin, or another message", () => {
    expect(readyFromFrame(event(other, origin, { type: PROPOSAL_READY }), frame, origin)).toBe(false)
    expect(readyFromFrame(event(frame, "https://evil.test", { type: PROPOSAL_READY }), frame, origin)).toBe(false)
    expect(readyFromFrame(event(frame, origin, { type: PROPOSAL }), frame, origin)).toBe(false)
    expect(readyFromFrame(event(frame, origin, "ready"), frame, origin)).toBe(false)
    expect(readyFromFrame(event(frame, origin, { type: PROPOSAL_READY }), null, origin)).toBe(false)
  })
})

describe("the frame takes a proposal only from the page around it", () => {
  const message = { type: PROPOSAL, document: doc, collections }

  it("keeps a renderable document from its parent on this origin", () => {
    expect(proposalFromParent(event(parent, origin, message), parent, origin, yes)).toEqual({
      document: doc,
      collections,
    })
  })

  it("ignores anything from another window, another origin or of another type", () => {
    expect(proposalFromParent(event(other, origin, message), parent, origin, yes)).toBeNull()
    expect(proposalFromParent(event(parent, "https://evil.test", message), parent, origin, yes)).toBeNull()
    expect(proposalFromParent(event(parent, origin, { ...message, type: PROPOSAL_READY }), parent, origin, yes)).toBeNull()
    expect(proposalFromParent(event(parent, origin, null), parent, origin, yes)).toBeNull()
  })

  it("marks a document the shared renderer cannot draw as invalid", () => {
    expect(proposalFromParent(event(parent, origin, message), parent, origin, no)).toBe("invalid")
    expect(proposalFromParent(event(parent, origin, { type: PROPOSAL }), parent, origin, yes)).toBe("invalid")
  })
})
