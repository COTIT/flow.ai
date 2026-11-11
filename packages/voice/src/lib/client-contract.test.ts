import { expect, it } from "vitest"
import { failureCategory } from "./client-contract"

it("buckets failures for monitoring; a dropped call is a platform fault", () => {
  expect(failureCategory("unsupported")).toBe("out_of_scope_candidate")
  expect(failureCategory("unverified")).toBe("platform_error")
  expect(failureCategory("dropped_disconnect")).toBe("platform_error")
  expect(failureCategory("dropped_silence")).toBe("platform_error")
  expect(failureCategory("invalid")).toBe("user_fixable")
})
