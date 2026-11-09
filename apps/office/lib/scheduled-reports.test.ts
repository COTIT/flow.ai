import { describe, expect, it } from "vitest"
import { timingWords, weekdayName } from "./scheduled-reports"

describe("scheduled report timing", () => {
  it("names ISO weekdays in each office language", () => {
    expect(weekdayName(1, "en")).toBe("Monday")
    expect(weekdayName(7, "en")).toBe("Sunday")
    expect(weekdayName(1, "nl")).toBe("maandag")
    expect(weekdayName(1, "sw")).toBe("Jumatatu")
  })

  it("words weekly, monthly and daily schedules with a zero-padded hour", () => {
    expect(
      timingWords(
        { frequency: "weekly", weekday: 1, day_of_month: null, hour: 7 },
        "en"
      )
    ).toEqual({
      key: "scheduled.every.weekly",
      values: { day: "Monday", time: "07:00" },
    })
    expect(
      timingWords(
        { frequency: "monthly", weekday: null, day_of_month: 28, hour: 23 },
        "en"
      )
    ).toEqual({
      key: "scheduled.every.monthly",
      values: { day: "28", time: "23:00" },
    })
    expect(
      timingWords(
        { frequency: "daily", weekday: null, day_of_month: null, hour: 6 },
        "en"
      ).key
    ).toBe("scheduled.every.daily")
  })
})
