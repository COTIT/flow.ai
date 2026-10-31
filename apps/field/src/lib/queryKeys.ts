/**
 * The query-key registry from business-rules §10.
 *
 * Not a stub: there is nothing to implement here, only the agreed shape. Every
 * `useQuery` takes its key from this file so that the invalidation map below
 * stays mechanical rather than remembered.
 *
 * Mutations invalidate the NARROWEST key that covers the change.
 *
 * Invalidation map (business-rules §10):
 *
 *   app_register_farmer      people(village), farms(village), tower.all(village)
 *   app_verify               the record key, tower.quality(village),
 *                            and the farmer-facing key for that record
 *   request insert / update  requests(...), request(id), estimate(id),
 *                            tower.energy(village)
 *   approve / reject         as above, plus the farmer's request(id)
 *   app_supersede_harvest    harvest(cycle), tower.production(village),
 *                            tower.market(village)
 *   opportunity_supply write opportunity(id), demand(demandId),
 *                            tower.market(village)
 *   app_farmer_login_issue   loginHistory(person)
 *   app_survey_submit        surveyEligibility, farmerVouchers, voucherTimeline(id)
 *   survey write / publish   surveyAdmin, surveyAdminDetail(id), surveyEligibility
 *   app_voucher_redeem       voucherTimeline(id), surveyVouchers(survey),
 *                            surveySummary, redemptionLog
 *   app_voucher_void         as redeem
 *   app_create_village       villageCapacity, villages, tower (all)
 *   plot_document upload     plotDocuments(plot)
 *   equipment insert         equipment('mine')
 *   loan_product insert      loanProducts
 */
export const queryKeys = {
  session: () => ['session'] as const,
  villages: () => ['villages'] as const,

  person: (personId: string) => ['person', personId] as const,
  people: (villageId: string, filters?: unknown) =>
    ['people', villageId, filters] as const,

  /**
   * Added beyond business-rules §10's list, which enumerated keys for the
   * screens it named and predates spec 5.1's officer home. Keyed by the whole
   * assigned-village set because the screen's figures are per membership, not
   * per village — an officer assigned to a second village must not read a
   * cached single-village answer.
   */
  officerHome: (villageIds: readonly string[]) =>
    ['officerHome', [...villageIds].sort()] as const,

  /**
   * Spec 5.7's verify queue, also beyond §10's list.
   *
   * NOT keyed by language, deliberately. It once was, because crop names come
   * from the database per locale — but the fix for QA #31 moved that choice to
   * render time everywhere, so the same rows serve every language from one
   * cache entry and a switch takes effect without a refetch.
   */
  verifyQueue: () => ['verifyQueue'] as const,

  /** Spec 7.1's ops home queue counts. */
  opsHome: () => ['opsHome'] as const,
  /** Spec 7.5's buyer list. */
  buyers: () => ['buyers'] as const,
  /** Spec 7.9's village list with its current capacity row. */
  villageCapacity: () => ['villageCapacity'] as const,
  /** A plot's title documents, with short-lived signed links. */
  plotDocuments: (plotId: string) => ['plotDocuments', plotId] as const,
  /** Loan listings in the resource catalogue. Listings only: no finance terms. */
  loanProducts: () => ['loanProducts'] as const,
  equipmentCategories: () => ['equipmentCategories'] as const,
  /** Spec 6.6's farmer-facing opportunities. Language is chosen at render. */
  farmerOpportunities: () => ['farmerOpportunities'] as const,

  /** A provenance actor's display name, from the scoped `app_actor_names`. */
  actorName: (actorId: string) => ['actorName', actorId] as const,

  farm: (farmId: string) => ['farm', farmId] as const,
  farms: (villageId: string) => ['farms', villageId] as const,

  cycle: (cycleId: string) => ['cycle', cycleId] as const,
  harvest: (cycleId: string) => ['harvest', cycleId] as const,

  equipment: (projectId: string) => ['equipment', projectId] as const,
  equipmentItem: (equipmentId: string) => ['equipmentItem', equipmentId] as const,

  requests: (filters: { villageId?: string; status?: string }) =>
    ['requests', filters] as const,
  request: (requestId: string) => ['request', requestId] as const,
  estimate: (requestId: string) => ['estimate', requestId] as const,

  demand: (demandId: string) => ['demand', demandId] as const,
  demands: (projectId: string) => ['demands', projectId] as const,
  opportunity: (opportunityId: string) => ['opportunity', opportunityId] as const,

  /**
   * Surveys and incentive vouchers — added beyond §10's list with the survey
   * feature (29 Sep 2026). Titles are chosen by language at render, never in
   * a queryFn.
   */
  /** The farmer's list: every visible survey with its eligibility and state. */
  surveyEligibility: () => ['surveyEligibility'] as const,
  /** One survey and its questions, as a farmer answers it. */
  survey: (surveyId: string) => ['survey', surveyId] as const,
  /** The household's own vouchers. */
  farmerVouchers: () => ['farmerVouchers'] as const,
  /** A voucher code: read through app_voucher_code, household only. */
  voucherCode: (voucherId: string) => ['voucherCode', voucherId] as const,
  /** The audit trail for one voucher, filtered by who is asking. */
  voucherTimeline: (voucherId: string) => ['voucherTimeline', voucherId] as const,
  /** The ops survey list, with its summary figures. */
  surveyAdmin: () => ['surveyAdmin'] as const,
  surveyAdminDetail: (surveyId: string) => ['surveyAdmin', surveyId] as const,
  surveyVouchers: (surveyId: string) => ['surveyVouchers', surveyId] as const,
  surveyTally: (surveyId: string) => ['surveyTally', surveyId] as const,
  /** Cash reconciliation: every redemption in a Tanzanian date range. */
  redemptionLog: (from: string, to: string) => ['redemptionLog', from, to] as const,
  /** Who issued or reset a person's app login. */
  loginHistory: (personId: string) => ['loginHistory', personId] as const,

  tower: {
    production: (villageId: string) => ['tower', 'production', villageId] as const,
    energy: (villageId: string) => ['tower', 'energy', villageId] as const,
    market: (villageId: string) => ['tower', 'market', villageId] as const,
    quality: (villageId: string) => ['tower', 'quality', villageId] as const,
  },
} as const

/**
 * `['tower', *, village]` in the invalidation map is a WILDCARD IN THE MIDDLE,
 * so it cannot be expressed as a key prefix: the village is the third element,
 * and `['tower', villageId]` matches nothing. Use a predicate.
 *
 *   queryClient.invalidateQueries({ predicate: isTowerQueryForVillage(v) })
 */
export function isTowerQueryForVillage(villageId: string) {
  return (query: { queryKey: readonly unknown[] }) =>
    query.queryKey[0] === 'tower' && query.queryKey[2] === villageId
}
