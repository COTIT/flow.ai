import { describe, expect, test } from 'vitest'

import { partitionEnergyRows, type EnergyRow } from '@/features/tower/energyRows'

const row = (status: EnergyRow['status'], kw: number | null, id: string = status): EnergyRow => ({
  id,
  status,
  applicant: 'Test Farmer',
  person_id: 'p1',
  equipment_name: 'Maize mill',
  est_power_kw: kw,
})

/**
 * QA-FINDINGS.md #7. The energy drill listed every request in the village —
 * draft and rejected included — under the heading "requests behind the energy
 * figures", beside headlines of 7.200 kW prospective and 10.800 kW approved
 * that those rows do not feed. Adding the column gave 42.5 kW, which
 * reconciled with nothing on the screen.
 *
 * Which statuses feed which figure is business-rules §7, not a UI choice:
 * prospective = submitted or under_review (an APPLICATION), approved =
 * approved (still not measured consumption). Everything else feeds neither.
 */
describe('partitionEnergyRows', () => {
  test('prospective is submitted and under_review, per business-rules §7', () => {
    const { prospective } = partitionEnergyRows([
      row('submitted', 5),
      row('under_review', 12),
      row('approved', 15),
      row('draft', 3),
      row('rejected', 7.5),
      row('withdrawn', 1.5),
    ])

    expect(prospective.map((r) => r.status)).toEqual(['submitted', 'under_review'])
  })

  test('approved is approved alone', () => {
    const { approved } = partitionEnergyRows([
      row('submitted', 5),
      row('approved', 15, 'a1'),
      row('approved', 3, 'a2'),
      row('rejected', 7.5),
    ])

    expect(approved.map((r) => r.id)).toEqual(['a1', 'a2'])
  })

  // Draft, rejected and withdrawn feed neither figure. They are separated out
  // rather than silently dropped, so the screen can say how many exist and
  // where to find them — hiding them would make the pipeline look smaller than
  // it is.
  test('everything else is excluded, and kept countable', () => {
    const { excluded } = partitionEnergyRows([
      row('draft', 3),
      row('rejected', 7.5),
      row('withdrawn', 1.5),
      row('approved', 15),
    ])

    expect(excluded.map((r) => r.status)).toEqual(['draft', 'rejected', 'withdrawn'])
  })

  test('every row lands in exactly one bucket', () => {
    const rows = [
      row('draft', 3),
      row('submitted', 5),
      row('under_review', 12),
      row('approved', 15),
      row('rejected', 7.5),
      row('withdrawn', 1.5),
    ]
    const { prospective, approved, excluded } = partitionEnergyRows(rows)

    expect(prospective.length + approved.length + excluded.length).toBe(rows.length)
    const ids = [...prospective, ...approved, ...excluded].map((r) => r.id)
    expect(new Set(ids).size).toBe(rows.length)
  })

  test('no rows partitions into three empty buckets', () => {
    expect(partitionEnergyRows([])).toEqual({ prospective: [], approved: [], excluded: [] })
  })

  // A request whose equipment has a null rated_power_kw has its estimate
  // DELETED by the trigger rather than zeroed (business-rules §3), so the row
  // survives with no estimate. It still belongs to its status bucket.
  test('a row with no estimate still lands in its bucket', () => {
    const { approved } = partitionEnergyRows([row('approved', null)])
    expect(approved).toHaveLength(1)
    expect(approved[0].est_power_kw).toBeNull()
  })
})
