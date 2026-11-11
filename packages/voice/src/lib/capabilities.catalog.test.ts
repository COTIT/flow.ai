import { describe, expect, it } from "vitest"

import catalog from "../../../../apps/office/lib/voice/catalog.generated.json" with { type: "json" }
import { FLOW_CAPABILITIES } from "./capabilities"

// The Flow registry is derived from the real tool catalog: it can neither invent nor drift.
const ops = new Map(catalog.map((op) => [op.id, op]))

describe("Flow capability registry vs the voice tool catalog", () => {
  it("names only operations that exist", () => {
    for (const cap of FLOW_CAPABILITIES)
      for (const id of [...cap.ops, ...(cap.after ?? [])])
        expect(ops.get(id), `${cap.id} → ${id}`).toBeDefined()
  })

  it("asks Genesy only for operations voice can run", () => {
    for (const cap of FLOW_CAPABILITIES.filter((c) => "prompt" in c.entry))
      for (const id of cap.ops) {
        const op = ops.get(id)!
        expect(op.disposition, `${cap.id} → ${id}`).toBe("execute")
        expect(op, `${cap.id} → ${id}`).not.toHaveProperty(
          "availability",
          "unavailable"
        )
      }
  })

  it("carries exactly the catalog's rights and write flag", () => {
    for (const cap of FLOW_CAPABILITIES) {
      const tools = cap.ops.map((id) => ops.get(id)!)
      for (const tool of tools) {
        expect(tool.authorization.anyOf, cap.id).toEqual([])
        expect(tool.authorization.roleAnyOf, cap.id).toEqual([])
      }
      expect([...cap.rights].sort(), cap.id).toEqual(
        [...new Set(tools.flatMap((t) => t.authorization.allOf))].sort()
      )
      expect(cap.write, cap.id).toBe(tools.some((t) => t.write))
    }
  })

  it("has no operation-less entry except planned ones and the explorer", () => {
    for (const cap of FLOW_CAPABILITIES.filter((c) => !c.ops.length))
      expect(cap.planned || cap.id === "explore", cap.id).toBeTruthy()
    for (const cap of FLOW_CAPABILITIES.filter((c) => c.planned))
      expect(cap.ops, cap.id).toEqual([])
  })

  it("ids are unique and every group has entries", () => {
    const ids = FLOW_CAPABILITIES.map((c) => c.id)
    expect(new Set(ids).size).toBe(ids.length)
    expect(new Set(FLOW_CAPABILITIES.map((c) => c.group))).toEqual(
      new Set(["understand", "operate", "automate", "create"])
    )
  })
})
