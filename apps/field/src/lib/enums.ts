/**
 * The closed value sets the Genesy API uses for RUAHA records (statuses, provenance, measures),
 * spelled exactly as the API sends them. Display-only: the API validates every value.
 */
export type Enums = {
  buyer_channel: 'direct' | 'afm' | 'other'
  capacity_basis: 'planned' | 'nameplate'
  confidence_level: 'low' | 'medium' | 'high'
  crop_cycle_status: 'planned' | 'growing' | 'harvested' | 'abandoned'
  crop_measure: 'area' | 'tree_count' | 'unit_count'
  demand_status: 'open' | 'matched' | 'closed' | 'cancelled'
  harvest_kind: 'expected' | 'actual'
  id_document_type: (typeof ID_DOCUMENT_TYPES)[number]
  opportunity_status: 'proposed' | 'shared' | 'accepted' | 'declined' | 'lapsed'
  pue_status: 'draft' | 'submitted' | 'under_review' | 'approved' | 'rejected' | 'withdrawn'
  source_type:
    | 'farmer_reported'
    | 'field_verified'
    | 'transaction_derived'
    | 'sensor_derived'
    | 'model_estimated'
  survey_status: 'draft' | 'live' | 'closed'
  verification_status: 'unverified' | 'pending' | 'verified' | 'disputed'
  voucher_status: 'issued' | 'redeemed' | 'void'
}

/** The four identity documents a voucher hand-over accepts. */
export const ID_DOCUMENT_TYPES = ['nida', 'voter', 'driving_licence', 'village_letter'] as const
