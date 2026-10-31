import { z } from 'zod'

import { hasAtMostDecimals } from '@/lib/decimals'

/**
 * The attach-supply form — spec 7.8, QA #21's tail.
 *
 * `opportunity_supply.contributed_kg` is
 * `numeric(12,2) not null check (contributed_kg > 0)`, so 0, a negative or a
 * figure past the column's scale came back as a raw constraint name. This
 * checks that column's own shape and nothing beyond it — including its scale,
 * because a third decimal is not refused by Postgres but rounded silently.
 *
 * **What it deliberately does not check.** Over-commitment.
 * `opportunity_supply_guard` owns that, its message names the actual
 * kilograms — "over-commitment: 4100.00 kg available, 4100.00 kg already
 * committed, 100.00 kg requested" — and business-rules §8 says to show it as
 * written. A client-side copy would drift, and would be wrong the instant
 * another opportunity committed the same harvest.
 *
 * Messages are i18n keys, resolved at render.
 */
export const supplySchema = z.object({
  harvest_report_id: z.string().min(1, 'opportunity.chooseHarvestRequired'),
  contributed_kg: z
    .string()
    .transform((s) => s.trim())
    .superRefine((value, ctx) => {
      if (value === '') {
        ctx.addIssue({ code: 'custom', message: 'opportunity.kgRequired' })
        return
      }

      const n = Number(value)
      if (!Number.isFinite(n)) {
        ctx.addIssue({ code: 'custom', message: 'opportunity.kgNotANumber' })
        return
      }
      if (n <= 0) {
        ctx.addIssue({ code: 'custom', message: 'opportunity.kgMoreThanZero' })
        return
      }
      // numeric(12,2) holds up to 10^10 − 0.01.
      if (n > 9_999_999_999.99) {
        ctx.addIssue({ code: 'custom', message: 'opportunity.kgTooLarge' })
        return
      }
      if (!hasAtMostDecimals(n, 2)) {
        ctx.addIssue({ code: 'custom', message: 'opportunity.kgDecimals' })
      }
    }),
})

export type SupplyForm = z.input<typeof supplySchema>
