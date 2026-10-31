import type { Database } from '@/lib/db.types'

type PueStatus = Database['public']['Enums']['pue_status']

export interface EnergyRow {
  id: string
  status: PueStatus
  applicant: string
  person_id: string | null
  equipment_name: string
  est_power_kw: number | null
}

export interface EnergyPartition {
  /** Feeds `prospective_peak_kw`. An application, not a load. */
  prospective: EnergyRow[]
  /** Feeds `approved_peak_kw`. Decided yes; still not measured consumption. */
  approved: EnergyRow[]
  /** Feeds neither figure. */
  excluded: EnergyRow[]
}

/**
 * Which statuses `v_village_energy` counts into which figure.
 *
 * Taken from the view's own `filter (where pr.status in (…))` clauses, not
 * invented here. If the view changes, these must change with it — which is
 * why the screen displays the view's figures rather than summing these rows.
 */
const PROSPECTIVE: readonly PueStatus[] = ['submitted', 'under_review']
const APPROVED: readonly PueStatus[] = ['approved']

/**
 * Splits a village's requests into the two energy figures they feed, and the
 * rest.
 *
 * QA-FINDINGS.md #7: the drill-down listed all six statuses under "requests
 * behind the energy figures" while the headlines counted only three of them,
 * so the column added to 42.5 kW against a screen showing 7.200 and 10.800.
 * Spec §8.2 makes traceability the Tower's licence to display a number; a
 * trace that does not reconcile is worse than none.
 *
 * This only groups. It deliberately does not total: business-rules §11 puts
 * village aggregates in the database, and `v_village_energy` already exposes
 * both the raw sums and the simultaneity-corrected peaks. The screen shows
 * those figures beside these rows.
 */
export function partitionEnergyRows(rows: EnergyRow[]): EnergyPartition {
  const prospective: EnergyRow[] = []
  const approved: EnergyRow[] = []
  const excluded: EnergyRow[] = []

  for (const row of rows) {
    if (PROSPECTIVE.includes(row.status)) prospective.push(row)
    else if (APPROVED.includes(row.status)) approved.push(row)
    else excluded.push(row)
  }

  return { prospective, approved, excluded }
}
