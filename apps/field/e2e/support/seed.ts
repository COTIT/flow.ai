/**
 * Fixed identifiers from supabase/seed.sql.
 *
 * These are part of the demo specification — CLAUDE.md asserts figures against
 * them — so referencing them is stable, unlike matching on a displayed name.
 * Crop names are translated IN THE DATABASE (name_en / name_sw), so what a
 * user sees depends on their app_user.locale: the seeded Ilundo officer is a
 * Swahili user and sees "Mahindi", not "Maize".
 */
export const VILLAGE = {
  ILUNDO: '30000000-0000-4000-8000-000000000001',
  MGAMA: '30000000-0000-4000-8000-000000000002',
} as const

export const CROP = {
  MAIZE: { id: '40000000-0000-4000-8000-000000000001', en: 'Maize', sw: 'Mahindi', measure: 'area' },
  AVOCADO: {
    id: '40000000-0000-4000-8000-000000000002',
    en: 'Avocado',
    sw: 'Parachichi',
    measure: 'tree_count',
  },
  HONEY: {
    id: '40000000-0000-4000-8000-000000000005',
    en: 'Honey',
    sw: 'Asali',
    measure: 'unit_count',
  },
} as const
