import { describe, expect, test } from 'vitest'

import { computeEstimate } from '@/features/farmer/energyEstimate'

/**
 * Mirrors energy_estimate's generated columns exactly:
 *   est_power_kw     = rated_power_kw * quantity
 *   est_kwh_per_day  = rated_power_kw * quantity * hours_per_day
 *   est_kwh_per_week = rated_power_kw * quantity * hours_per_day * days_per_week
 *
 * If the two ever disagree, the component is wrong — not the database.
 */
describe('computeEstimate', () => {
  test('the seeded maize mill, at its typical assumptions', () => {
    // MILL-500: 15 kW, 6 h/day, 5 days/week.
    const e = computeEstimate({ ratedPowerKw: 15, quantity: 1, hoursPerDay: 6, daysPerWeek: 5 })
    expect(e.estPowerKw).toBe(15)
    expect(e.estKwhPerDay).toBe(90)
    expect(e.estKwhPerWeek).toBe(450)
  })

  // The acceptance criterion for /farm/equipment/$equipmentId.
  test('changing hours from 6 to 8 changes the daily and weekly figures', () => {
    const six = computeEstimate({ ratedPowerKw: 15, quantity: 1, hoursPerDay: 6, daysPerWeek: 5 })
    const eight = computeEstimate({ ratedPowerKw: 15, quantity: 1, hoursPerDay: 8, daysPerWeek: 5 })

    expect(eight.estKwhPerDay).toBe(120)
    expect(eight.estKwhPerWeek).toBe(600)
    // Peak power does not move with hours — that distinction is the whole
    // point of keeping kW and kWh apart.
    expect(eight.estPowerKw).toBe(six.estPowerKw)
  })

  test('quantity multiplies peak power as well as energy', () => {
    // PUMP-2HP at quantity 2: the seeded approved pair.
    const e = computeEstimate({ ratedPowerKw: 1.5, quantity: 2, hoursPerDay: 5, daysPerWeek: 6 })
    expect(e.estPowerKw).toBe(3)
    expect(e.estKwhPerDay).toBe(15)
    expect(e.estKwhPerWeek).toBe(90)
  })

  test('rounds to three decimals, as the numeric(12,3) columns do', () => {
    const e = computeEstimate({ ratedPowerKw: 1.234, quantity: 3, hoursPerDay: 2.5, daysPerWeek: 6.5 })
    expect(e.estPowerKw).toBe(3.702)
    expect(e.estKwhPerDay).toBe(9.255)
    expect(e.estKwhPerWeek).toBe(60.158)
  })

  test('zero hours is a real answer: capacity but no consumption', () => {
    const e = computeEstimate({ ratedPowerKw: 15, quantity: 1, hoursPerDay: 0, daysPerWeek: 5 })
    expect(e.estPowerKw).toBe(15)
    expect(e.estKwhPerDay).toBe(0)
    expect(e.estKwhPerWeek).toBe(0)
  })

  test('a cold room runs continuously without overflowing the week', () => {
    // COLD-5: 5 kW, 24 h/day, 7 days/week.
    const e = computeEstimate({ ratedPowerKw: 5, quantity: 1, hoursPerDay: 24, daysPerWeek: 7 })
    expect(e.estKwhPerDay).toBe(120)
    expect(e.estKwhPerWeek).toBe(840)
  })

  // The trigger coalesces a missing assumption to the equipment's typical
  // value, then to 0. The preview must not show NaN while a field is empty.
  test('blank assumptions read as zero rather than NaN', () => {
    const e = computeEstimate({
      ratedPowerKw: 15,
      quantity: Number.NaN,
      hoursPerDay: Number.NaN,
      daysPerWeek: Number.NaN,
    })
    expect(e.estPowerKw).toBe(0)
    expect(e.estKwhPerDay).toBe(0)
    expect(e.estKwhPerWeek).toBe(0)
  })

  test('a negative input cannot produce a negative estimate', () => {
    const e = computeEstimate({ ratedPowerKw: -5, quantity: -1, hoursPerDay: -2, daysPerWeek: -3 })
    expect(e.estPowerKw).toBe(0)
    expect(e.estKwhPerDay).toBe(0)
    expect(e.estKwhPerWeek).toBe(0)
  })
})
