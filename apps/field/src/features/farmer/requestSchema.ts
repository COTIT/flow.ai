import { z } from 'zod'

import { hasAtMostDecimals } from '@/lib/decimals'

/**
 * The equipment request form — spec 6.4, QA #9.
 *
 * The finding's table is the specification here: 99 hours per day rendered a
 * confident 1,485 kWh/day, zero hours rendered 0 kWh with submit still
 * enabled, and quantity 0 rendered a 0 kW peak the database then refused. The
 * farmer was shown a computed, impossible figure and learnt it was impossible
 * only after submitting.
 *
 * `quantity integer not null check (quantity > 0)` and
 * `hours_per_day numeric(4,2) check (hours_per_day between 0 and 24)` are the
 * columns' own shapes, so bounding the inputs to them is not a copy of a
 * business rule — the alternative is a round trip returning
 * `pue_request_hours_per_day_check` as user copy (QA #4).
 *
 * **One place where this is deliberately STRICTER than the database:** zero
 * hours and zero days. The column permits both. A request to run a mill for
 * zero hours asks for nothing, estimates 0 kWh, and would sit in the ops
 * pipeline as a decision nobody can make. #9 asks for it, and refusing to
 * send something is a different act from claiming a rule the server does not
 * have.
 *
 * **Decimal places.** `hours_per_day numeric(4,2)` and
 * `days_per_week numeric(3,1)` do not refuse a value past their scale — they
 * round it silently, so 2.25 days would be stored as 2.3 without the farmer
 * being told (and the retry reconciliation would then see a stored figure that
 * differs from the submitted one). The form refuses what the column would
 * change.
 *
 * Messages are i18n keys, resolved at render.
 */

interface Bounds {
  min: number
  max: number
  /** Message key when outside [min, max]. */
  rangeKey: string
  integer?: boolean
  /** The column's scale, with the message key when a value exceeds it. */
  decimals?: { places: number; key: string }
}

/**
 * A required numeric input, kept as text.
 *
 * All three feed the estimate, so a blank is not "no answer" — `Number('')` is
 * 0, and `energy_estimate` needs all three to compute anything.
 */
function requiredNumber({ min, max, rangeKey, integer = false, decimals }: Bounds) {
  return z
    .string()
    .transform((s) => s.trim())
    .superRefine((value, ctx) => {
      if (value === '') {
        ctx.addIssue({ code: 'custom', message: 'equipment.required' })
        return
      }

      const n = Number(value)
      if (!Number.isFinite(n)) {
        ctx.addIssue({ code: 'custom', message: 'equipment.notANumber' })
        return
      }
      if (integer && !Number.isInteger(n)) {
        ctx.addIssue({ code: 'custom', message: 'equipment.wholeNumber' })
        return
      }
      // Zero and below is its own message: "between 0 and 24" is true of 0,
      // so calling it out of range would be wrong about why.
      if (n <= 0) {
        ctx.addIssue({ code: 'custom', message: 'equipment.moreThanZero' })
        return
      }
      if (n < min || n > max) {
        ctx.addIssue({ code: 'custom', message: rangeKey })
        return
      }
      if (decimals && !hasAtMostDecimals(n, decimals.places)) {
        ctx.addIssue({ code: 'custom', message: decimals.key })
      }
    })
}

export const requestSchema = z.object({
  quantity: requiredNumber({
    min: 1,
    // `integer`, so the column's own ceiling.
    max: 2_147_483_647,
    rangeKey: 'equipment.moreThanZero',
    integer: true,
  }),
  // numeric(4,2)
  hours_per_day: requiredNumber({
    min: 0,
    max: 24,
    rangeKey: 'equipment.hoursRange',
    decimals: { places: 2, key: 'equipment.hoursDecimals' },
  }),
  // numeric(3,1)
  days_per_week: requiredNumber({
    min: 0,
    max: 7,
    rangeKey: 'equipment.daysRange',
    decimals: { places: 1, key: 'equipment.daysDecimals' },
  }),
  /** `purpose` is nullable text. Trimmed, so `"   "` is stored as nothing. */
  purpose: z.string().transform((s) => s.trim()),
})

export type RequestForm = z.input<typeof requestSchema>
