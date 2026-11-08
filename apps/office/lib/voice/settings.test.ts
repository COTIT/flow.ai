import { afterEach, expect, it, vi } from "vitest"
import { voiceSetting } from "./settings"

vi.mock("node:fs/promises", () => ({ readFile: vi.fn() }))
afterEach(() => {
  vi.unstubAllEnvs()
  vi.restoreAllMocks()
})

it("reads runtime secret files and permits an empty optional tracing key", async () => {
  const { readFile } = await import("node:fs/promises")
  vi.stubEnv("LANGSMITH_API_KEY", "")
  vi.stubEnv("LANGSMITH_API_KEY_FILE", "/run/secrets/tracing")
  vi.mocked(readFile)
    .mockResolvedValueOnce("secret\r\n")
    .mockResolvedValueOnce("")
  expect(await voiceSetting("LANGSMITH_API_KEY")).toBe("secret")
  expect(await voiceSetting("LANGSMITH_API_KEY")).toBe("")
})

it("refuses conflicting secrets and sanitizes file read failures", async () => {
  const { readFile } = await import("node:fs/promises")
  vi.stubEnv("ELEVENLABS_API_KEY", "secret")
  vi.stubEnv("ELEVENLABS_API_KEY_FILE", "/private/secret-provider")
  await expect(voiceSetting("ELEVENLABS_API_KEY")).rejects.toThrow(
    "Conflicting ELEVENLABS_API_KEY configuration"
  )
  vi.stubEnv("ELEVENLABS_API_KEY", "")
  vi.mocked(readFile).mockRejectedValue(
    new Error("/private/secret-provider: secret contents")
  )
  await expect(voiceSetting("ELEVENLABS_API_KEY")).rejects.toThrow(
    /^ELEVENLABS_API_KEY file is unreadable$/
  )
})
