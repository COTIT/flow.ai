import { describe, expect, it } from "vitest"

import { discoverTools } from "./catalog"
import heals from "./gap-heal.fixture.json"

// Every trigger-word fix `voice:heal` committed: the failed request must keep finding its operation.
describe("healed voice gaps", () => {
  it("still finds the operation for every healed request", () => {
    const perms = ["workspace_owner", "farm:admin", "farm:ops"]
    const missed = (heals as { intent: string; want: string }[]).filter(
      ({ intent, want }) =>
        !discoverTools(intent, perms)
          .slice(0, 3)
          .some((tool) => tool.id === want)
    )
    expect(missed).toEqual([])
  })
})
