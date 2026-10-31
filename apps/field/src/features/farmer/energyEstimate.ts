export interface EstimateInputs {
  ratedPowerKw: number
  quantity: number
  hoursPerDay: number
  daysPerWeek: number
}

export interface Estimate {
  estPowerKw: number
  estKwhPerDay: number
  estKwhPerWeek: number
}

/**
 * Half-up division of an integer by a power of ten, matching how Postgres
 * rounds a cast to numeric(12,3).
 */
function scaleDown(value: number, divisor: number): number {
  return Math.floor((value + divisor / 2) / divisor) / 1000
}

/**
 * A blank or nonsensical input reads as zero rather than NaN.
 *
 * The trigger coalesces a missing assumption to the equipment's typical value
 * and then to 0, and every column carries a `>= 0` check, so a negative can
 * never be stored either. The preview must not show NaN while someone is
 * mid-edit.
 */
function clamp(value: number): number {
  if (!Number.isFinite(value) || value < 0) return 0
  return value
}

/**
 * The Level 1 estimate — rated power times operating assumptions (Plan S10).
 *
 * Mirrors energy_estimate's generated columns exactly:
 *   est_power_kw     = rated_power_kw * quantity
 *   est_kwh_per_day  = rated_power_kw * quantity * hours_per_day
 *   est_kwh_per_week = rated_power_kw * quantity * hours_per_day * days_per_week
 *
 * Computed in SCALED INTEGERS, not floats. Postgres numeric is exact decimal
 * and rounds half-up, so 1.234 x 3 x 2.5 x 6.5 stores as 60.158; the same
 * expression in binary floating point lands on 60.157499… and rounds to
 * 60.157. The column types fix the scales — rated_power_kw is numeric(8,3),
 * hours numeric(4,2), days numeric(3,1) — so working in thousandths,
 * hundredths and tenths is exact.
 *
 * If this and the database ever disagree, this is what is wrong. The client
 * computes it only to preview: the stored figure is written by the trigger,
 * which is the sole writer of energy_estimate.
 *
 * Note kW does not move with hours. Peak power and consumption are different
 * quantities and the surfaces must never blur them.
 */
export function computeEstimate(inputs: EstimateInputs): Estimate {
  const ratedMilli = Math.round(clamp(inputs.ratedPowerKw) * 1000)
  const quantity = Math.round(clamp(inputs.quantity))
  const hoursCenti = Math.round(clamp(inputs.hoursPerDay) * 100)
  const daysDeci = Math.round(clamp(inputs.daysPerWeek) * 10)

  // kW in thousandths
  const powerMilli = ratedMilli * quantity
  // kWh/day in hundred-thousandths
  const dayScaled = powerMilli * hoursCenti
  // kWh/week in millionths
  const weekScaled = dayScaled * daysDeci

  return {
    estPowerKw: powerMilli / 1000,
    estKwhPerDay: scaleDown(dayScaled, 100),
    estKwhPerWeek: scaleDown(weekScaled, 1000),
  }
}
