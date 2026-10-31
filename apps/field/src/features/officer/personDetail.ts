import type { Database } from '@/lib/db.types'

type Verification = Database['public']['Enums']['verification_status']
type Source = Database['public']['Enums']['source_type']
type Confidence = Database['public']['Enums']['confidence_level']

/** The provenance block every observed record carries. */
export interface Provenance {
  source: Source
  verification: Verification
  confidence: Confidence | null
  captured_at: string
  captured_by: string | null
  verified_by: string | null
  verified_at: string | null
}

export interface PersonDetail {
  person: Provenance & {
    id: string
    given_name: string
    family_name: string
    phone: string | null
    village_id: string
  }
  households: Array<
    Provenance & {
      id: string
      label: string
      members: Array<{ id: string; given_name: string; family_name: string }>
    }
  >
  farms: Array<
    Provenance & {
      id: string
      label: string
      latitude: number | null
      longitude: number | null
      plots: Array<
        Provenance & {
          id: string
          label: string
          area_ha: number | null
          latitude: number | null
          longitude: number | null
          cycles: Array<
            Provenance & {
              id: string
              crop_id: string
              crop_name: string
              season_label: string | null
              area_ha: number | null
              tree_count: number | null
              unit_count: number | null
              planted_on: string | null
              harvest_start: string | null
              harvest_end: string | null
              status: Database['public']['Enums']['crop_cycle_status']
              harvests: Array<
                Provenance & {
                  id: string
                  kind: Database['public']['Enums']['harvest_kind']
                  quantity_kg: number
                  is_current: boolean
                  reported_for: string | null
                }
              >
            }
          >
        }
      >
    }
  >
}

/**
 * Exactly the tables app_verify accepts. Anything else raises
 * "not a verifiable table: %", so the UI must not offer one.
 */
export const VERIFIABLE_TABLES = [
  'person',
  'household',
  'farm',
  'plot',
  'crop_cycle',
  'harvest_report',
] as const

export type VerifiableTable = (typeof VERIFIABLE_TABLES)[number]

export function isVerifiable(table: string): table is VerifiableTable {
  return (VERIFIABLE_TABLES as readonly string[]).includes(table)
}

export interface VerifiableRecord {
  table: VerifiableTable
  id: string
  verification: Verification
  label: string
}

/**
 * Every record on this screen that an officer could verify, in the order it is
 * rendered: person, then households, then farm -> plot -> cycle -> harvest.
 */
export function verifiableRecords(detail: PersonDetail): VerifiableRecord[] {
  const records: VerifiableRecord[] = [
    {
      table: 'person',
      id: detail.person.id,
      verification: detail.person.verification,
      label: `${detail.person.given_name} ${detail.person.family_name}`,
    },
  ]

  for (const household of detail.households) {
    records.push({
      table: 'household',
      id: household.id,
      verification: household.verification,
      label: household.label,
    })
  }

  for (const farm of detail.farms) {
    records.push({ table: 'farm', id: farm.id, verification: farm.verification, label: farm.label })
    for (const plot of farm.plots) {
      records.push({ table: 'plot', id: plot.id, verification: plot.verification, label: plot.label })
      for (const cycle of plot.cycles) {
        records.push({
          table: 'crop_cycle',
          id: cycle.id,
          verification: cycle.verification,
          label: cycle.crop_name,
        })
        for (const harvest of cycle.harvests) {
          records.push({
            table: 'harvest_report',
            id: harvest.id,
            verification: harvest.verification,
            label: `${harvest.kind} ${harvest.quantity_kg}`,
          })
        }
      }
    }
  }

  return records
}

/**
 * How many records still need an officer.
 *
 * 'unverified' and 'pending' both count. 'disputed' does not: it is a flagged
 * disagreement to resolve, not outstanding verification work, and rolling it
 * in would hide it inside a queue count.
 */
export function countUnverified(detail: PersonDetail): number {
  return verifiableRecords(detail).filter(
    (r) => r.verification === 'unverified' || r.verification === 'pending',
  ).length
}
