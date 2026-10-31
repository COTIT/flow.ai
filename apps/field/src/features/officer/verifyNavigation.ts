import type { QueueRow } from '@/features/officer/useVerifyQueue'

export type VerifyTarget =
  | { to: '/officer/people/$personId'; params: { personId: string }; search?: undefined }
  | { to: '/officer/farms/$farmId'; params: { farmId: string }; search?: { plot?: string } }
  | { to: '/officer/cycles/$cycleId'; params: { cycleId: string }; search?: { harvest?: string } }

export function getVerifyTarget(row: QueueRow): VerifyTarget | null {
  switch (row.table) {
    case 'person':
      return { to: '/officer/people/$personId', params: { personId: row.id } }
    case 'farm':
      return { to: '/officer/farms/$farmId', params: { farmId: row.id } }
    case 'plot':
      return row.farm_id
        ? { to: '/officer/farms/$farmId', params: { farmId: row.farm_id }, search: { plot: row.id } }
        : null
    case 'crop_cycle':
      return { to: '/officer/cycles/$cycleId', params: { cycleId: row.id } }
    case 'harvest_report':
      return row.crop_cycle_id
        ? {
            to: '/officer/cycles/$cycleId',
            params: { cycleId: row.crop_cycle_id },
            search: { harvest: row.id },
          }
        : null
    default:
      return null
  }
}
