import { afterEach, expect, it, vi } from "vitest"
import { requireAssistantAdmission } from "./admission"

afterEach(() => vi.unstubAllEnvs())
it.each(["false", "", "yes", "TRUE"])(
  "refuses new work when enabled flag is %j",
  async (value) => {
    vi.stubEnv("ASSISTANT_ENABLED", value)
    await expect(requireAssistantAdmission()).rejects.toMatchObject({
      status: 503,
    })
  }
)
it("admits only explicitly enabled work", async () => {
  vi.stubEnv("ASSISTANT_ENABLED", "true")
  await expect(requireAssistantAdmission()).resolves.toBeUndefined()
})

const workspace = "01950000-0000-7000-8000-000000000009"
it("limits new work to configured workspaces despite ordinary membership", async () => {
  vi.stubEnv("ASSISTANT_ENABLED", "true")
  vi.stubEnv("ASSISTANT_WORKSPACE_ALLOWLIST", workspace)
  await expect(requireAssistantAdmission(workspace)).resolves.toBeUndefined()
  await expect(
    requireAssistantAdmission("01950000-0000-7000-8000-000000000008")
  ).rejects.toMatchObject({ status: 403 })
})
it.each([
  "",
  "*",
  "not-a-workspace",
  `${workspace},invalid`,
  Array.from({ length: 101 }, () => workspace).join(","),
])("refuses incomplete or invalid rollout configuration %j", async (value) => {
  vi.stubEnv("ASSISTANT_ENABLED", "true")
  vi.stubEnv("ASSISTANT_WORKSPACE_ALLOWLIST", value)
  await expect(requireAssistantAdmission(workspace)).rejects.toMatchObject({
    status: 503,
  })
})
