import { expect, it } from "vitest"
import { timeZoneUpdate } from "./time-zone"

it("tells the agent the workspace zone with its offset and to always name it", () => {
  const update = JSON.parse(
    timeZoneUpdate("Africa/Dar_es_Salaam", new Date("2026-11-08T12:00:00Z"))
  )
  expect(update.workspace_time_zone).toBe("Africa/Dar_es_Salaam")
  expect(update.utc_offset).toBe("GMT+03:00")
  expect(update.instruction).toMatch(/UTC/)
  expect(update.instruction).toMatch(/never guess/i)
})

it("says UTC when the workspace zone is not known, never the device zone", () => {
  const update = JSON.parse(timeZoneUpdate(undefined))
  expect(update.workspace_time_zone).toBe("UTC")
  expect(update.utc_offset).toBe("GMT")
})
