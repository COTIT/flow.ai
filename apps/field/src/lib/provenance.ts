/**
 * Provenance — business-rules §4. Write this before the second feature, not
 * after the thirtieth call site.
 */

import type { Database } from '@/lib/db.types'

/** The five S13 source categories, straight from the database enum. */
export type SourceType = Database['public']['Enums']['source_type']

/**
 * Tables carrying the full provenance block. Every insert into one of these
 * goes through withProvenance().
 */
export const OBSERVED_TABLES = [
  'person',
  'household',
  'farm',
  'plot',
  'crop_cycle',
  'harvest_report',
  'pue_request',
] as const

/**
 * Tables with NO provenance columns. Setting them is an error, not a no-op.
 */
export const CONFIG_TABLES = [
  'country',
  'project',
  'village',
  'crop',
  'equipment_category',
  'equipment',
  'buyer',
  'village_capacity',
] as const

/**
 * `buyer_demand` is in neither list, on purpose. The five source categories
 * classify how a fact about the *productive economy* was learned; a buyer's
 * stated requirement is a counterparty input recorded by ops, so the table
 * carries captured_by / verification but no `source`.
 */

export type ObservedTable = (typeof OBSERVED_TABLES)[number]
export type ConfigTable = (typeof CONFIG_TABLES)[number]

/**
 * Which source to use, by writer (business-rules §4):
 *
 *   officer registering or editing a record   field_verified
 *   a number the farmer supplied, typed by
 *     anyone                                  farmer_reported
 *   a farmer editing their own profile        farmer_reported
 *   a PUE request                             farmer_reported (schema default)
 *   an energy estimate                        model_estimated (trigger sets it)
 *   meter data                                sensor_derived  — NOT IN MVP
 *   a recorded transaction                    transaction_derived — NOT IN MVP
 *
 * Nothing in the MVP writes sensor_derived or transaction_derived.
 */

export interface ProvenanceStamp {
  source: SourceType
  captured_at: string
  captured_by: string
}

/**
 * Stamps an observed-table insert with where the data came from.
 *
 * The stamp is applied LAST, so a payload cannot smuggle its own `source` or
 * `captured_by` past it — a client that can assert who captured a record can
 * launder provenance. The payload is copied rather than mutated.
 *
 * `verification` is deliberately absent: it stays at the column default of
 * 'unverified' and only moves through app_verify(), so nothing can be marked
 * verified without a verifier.
 *
 * Prefer the bound two-argument form from useProvenance() at call sites.
 */
export function withProvenance<T extends object>(
  payload: T,
  source: SourceType,
  capturedBy: string,
  capturedAt: string = new Date().toISOString(),
): T & ProvenanceStamp {
  return {
    ...payload,
    source,
    captured_at: capturedAt,
    captured_by: capturedBy,
  }
}
