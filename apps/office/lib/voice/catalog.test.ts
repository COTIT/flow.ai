import { describe, expect, it } from "vitest"
import { execFileSync } from "node:child_process"

import { discoverTools, prepareAction, tools } from "./catalog"

const workspace = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"

describe("voice tool boundary", () => {
  it("keeps tool contracts synchronized with the backend", () => {
    expect(() =>
      execFileSync(
        process.execPath,
        ["scripts/generate-voice-catalog.mjs", "--check"],
        { cwd: process.cwd(), stdio: "pipe" }
      )
    ).not.toThrow()
  })
  it("discovers business operations with their request schema", () => {
    const tool = discoverTools("orders_create", ["workspace_owner"])[0]!
    expect(tool.id).toBe("orders_create")
    expect(tool.write).toBe(true)
    expect(tool.body).toHaveProperty("properties.lines")
  })

  it("finds the invite as the way to give someone a team role", () => {
    expect(discoverTools("invite manager role", ["workspace_owner"])[0]?.id).toBe(
      "invites_send"
    )
    expect(
      discoverTools("team role permissions", ["workspace_owner"]).map((t) => t.id)
    ).toContain("teams_list")
  })

  it("offers member removal to admins only, as a reviewed write", () => {
    const tool = discoverTools("remove member access", ["workspace_owner"])[0]!
    expect(tool.id).toBe("users_remove")
    expect(tool.write).toBe(true)
    expect(discoverTools("users_remove", ["user.get", "invite.delete"])).toEqual([])
  })

  it("does not expose authentication or credential administration", () => {
    expect(() => prepareAction("auth_login", {}, workspace)).toThrow()
    expect(() => prepareAction("app_keys_create", {}, workspace)).toThrow()
  })

  it("binds workspace and credentials outside model arguments", () => {
    expect(() =>
      prepareAction("orders_list", { path: { workspace: "other" } }, workspace)
    ).toThrow()
    expect(() =>
      prepareAction(
        "orders_list",
        { headers: { Authorization: "other" } },
        workspace
      )
    ).toThrow()
    const call = prepareAction(
      "orders_get",
      { path: { order: workspace } },
      workspace
    )
    expect(call.path).toBe(`/workspace/${workspace}/orders/${workspace}`)
  })

  it("refuses traversal and unknown filters", () => {
    expect(() =>
      prepareAction("orders_get", { path: { order: ".." } }, workspace)
    ).toThrow()
    expect(() =>
      prepareAction("orders_list", { query: { arbitrary: "x" } }, workspace)
    ).toThrow()
  })

  it("bounds list reads and identifies supported idempotency", () => {
    expect(prepareAction("orders_list", {}, workspace).path).toContain(
      "page_size=20"
    )
    expect(
      prepareAction(
        "orders_create",
        { body: { lines: [{ product_id: workspace, quantity: "1" }] } },
        workspace
      ).idempotent
    ).toBe(true)
    expect(tools.find((tool) => tool.id === "messages_send")?.idempotent).toBe(
      false
    )
  })
})
