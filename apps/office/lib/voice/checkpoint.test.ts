import { afterEach, beforeEach, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  pool: vi.fn(),
  connect: vi.fn(),
  query: vi.fn(),
  release: vi.fn(),
}))
vi.mock("pg", () => ({
  default: {
    Pool: class {
      constructor() {
        mocks.pool()
      }
      connect = mocks.connect
      query = mocks.query
    },
  },
}))

beforeEach(() => {
  delete (globalThis as { __genesyVoiceCheckpoint?: unknown })
    .__genesyVoiceCheckpoint
  vi.resetModules()
  vi.clearAllMocks()
  vi.stubEnv(
    "ASSISTANT_CHECKPOINT_URL",
    "postgres://test:test@127.0.0.1/isolated"
  )
  mocks.query.mockResolvedValue({ rows: [{ locked: true, version: 5 }] })
  mocks.connect.mockResolvedValue({
    query: mocks.query,
    release: mocks.release,
  })
})
afterEach(() => vi.unstubAllEnvs())

it("shares one process pool across separately loaded Next route modules", async () => {
  const first = await import("./checkpoint")
  await first.withVoiceThread("s1", async () => undefined)
  vi.resetModules()
  const second = await import("./checkpoint")
  await second.withVoiceThread("s2", async () => undefined)
  expect(mocks.pool).toHaveBeenCalledTimes(1)
})

it("leaves pool capacity for checkpoint writes while two graph locks are held", async () => {
  const { withVoiceThread } = await import("./checkpoint")
  let finish = () => {}
  const gate = new Promise<void>((resolve) => {
    finish = resolve
  })
  const action = vi.fn(() => gate)
  const first = withVoiceThread("one", action)
  const second = withVoiceThread("two", action)
  await vi.waitFor(() => expect(action).toHaveBeenCalledTimes(2))
  await expect(withVoiceThread("three", action)).rejects.toMatchObject({
    status: 429,
  })
  finish()
  await Promise.all([first, second])
  await withVoiceThread("four", async () => undefined)
  expect(mocks.release).toHaveBeenCalledTimes(3)
})
