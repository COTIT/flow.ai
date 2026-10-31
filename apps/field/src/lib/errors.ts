/**
 * What a user reads when a write fails — QA #3, #4, #20, #25.
 *
 * **The distinction this module holds.** business-rules §9 says the Postgres
 * messages in THIS schema are written to be read by humans, and must be
 * surfaced verbatim. `over-commitment: 4100.00 kg available, 4100.00 kg
 * already committed, 100.00 kg requested` names the numbers the user needs;
 * rewriting it removes the only part worth reading. `this crop is measured by
 * area: area_ha is required` belongs on the field, as written.
 *
 * What §9 never covers is the machine noise underneath those: constraint
 * identifiers, type-parse failures, overflow, and JS exceptions. Those reached
 * users raw — `pue_request_hours_per_day_check` to a farmer, in a language she
 * does not read — and they are the only thing mapped here.
 *
 * **One narrow exception, for Swahili.** A message the app can name EXACTLY is
 * shown from the i18n bundle, carrying every value it had (`dbMessages.ts`).
 * A user who does not read English cannot use a sentence that is right and in
 * the wrong language. Anything not named exactly is still verbatim.
 *
 * **So the default is verbatim.** A message is replaced only when it is
 * recognised as something no human wrote. An unfamiliar sentence is far more
 * likely to be one of ours than machine noise, and a sentence someone wrote
 * beats "something went wrong".
 */

import { translateDbMessage } from '@/lib/dbMessages'

export type HumanError =
  /** One of the schema's own messages. Show as written. */
  | { kind: 'verbatim'; message: string }
  /**
   * Machine noise, replaced, or a message the app names exactly. `key` is an
   * i18n key, resolved at render with `values` when the message carried any.
   */
  | { kind: 'key'; key: string; values?: Record<string, string> }

/** Check constraints, by the identifier Postgres puts in the message. */
const CHECK_CONSTRAINT: Record<string, string> = {
  pue_request_hours_per_day_check: 'error.hoursRange',
  pue_request_days_per_week_check: 'error.daysRange',
  pue_request_quantity_check: 'error.quantityPositive',
  energy_estimate_hours_per_day_check: 'error.hoursRange',
  energy_estimate_days_per_week_check: 'error.daysRange',
  energy_estimate_quantity_check: 'error.quantityPositive',
  cycle_window_sane: 'error.windowBackwards',
  demand_window_sane: 'error.windowBackwards',
  opportunity_supply_contributed_kg_check: 'error.contributionPositive',
  buyer_demand_quantity_kg_check: 'error.quantityPositive',
  harvest_report_quantity_kg_check: 'error.quantityNotNegative',
  equipment_offered: 'error.equipmentOffered',
  loan_product_amount_range: 'error.loanRange',
}

/**
 * Unique constraints worth naming.
 *
 * The generic fallback ("that record already exists") is true but unhelpful
 * when the app can say WHICH collision it was. Postgres names the constraint,
 * not the value, so this is the only place that knowledge can live.
 */
const UNIQUE_CONSTRAINT: Record<string, string> = {
  // §9's row: you wrote a harvest row directly instead of superseding.
  harvest_one_current: 'error.oneCurrentHarvest',
  buyer_project_id_name_key: 'error.duplicateBuyer',
  opportunity_supply_opportunity_id_harvest_report_id_key: 'error.duplicateSupply',
  equipment_project_id_code_key: 'error.duplicateEquipmentCode',
  village_project_id_code_key: 'error.duplicateVillageCode',
  loan_product_project_id_code_key: 'error.duplicateLoanCode',
  // One try per household per survey: the backstop behind app_survey_submit.
  survey_response_one_per_household: 'error.surveyAlreadyAnswered',
  app_user_person_unique: 'error.personHasLogin',
}

/**
 * Browsers word a failed request differently and none of the wordings is copy.
 * Chrome/Firefox/Safari respectively, plus React Native's.
 */
const NETWORK = /failed to fetch|networkerror|network request failed|load failed/i

/** A stringified JS exception, e.g. "TypeError: x is not a function". */
const JS_EXCEPTION = /^(Type|Reference|Syntax|Range|Eval|URI|Internal)Error\b/

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message: unknown }).message
    if (typeof message === 'string') return message
  }
  return ''
}

/**
 * Conditions where the same request would probably succeed a moment later.
 *
 * Deliberately short. `retry: false` is the app's default because almost every
 * failure here is a decision the database made — a policy, a constraint, a
 * guard — and repeating the request repeats the decision while delaying the
 * message the user needs.
 *
 * Two exceptions earn a retry:
 *
 *   the connection   a village with intermittent signal, which is the whole
 *                    deployment context
 *   JWT timing       GoTrue can mint a token a fraction ahead of the clock
 *                    that validates it, and the first request carrying it is
 *                    rejected for skew (QA #33). A second later it is fine.
 *
 * Note what this is NOT about. CLAUDE.md's "zero rows is an answer … never
 * retry" concerns an EMPTY RESULT, which is a success: it never reaches this
 * function, and no retry policy has ever applied to it.
 */
const TRANSIENT = /^JWT (issued at future|expired)|failed to fetch|networkerror|network request failed|load failed/i

export function isTransientError(error: unknown): boolean {
  const message = messageOf(error).trim()
  return message !== '' && TRANSIENT.test(message)
}

export function humanizeDbError(error: unknown): HumanError {
  const message = messageOf(error).trim()

  // Nothing to show. A blank banner is worse than an admission.
  if (message === '') return { kind: 'key', key: 'error.unexpected' }

  // Connection first: a TypeError from fetch is the single most common
  // failure in a village with intermittent signal, and it is not a database
  // error at all.
  if (NETWORK.test(message)) return { kind: 'key', key: 'error.network' }

  const check = /violates check constraint "([^"]+)"/.exec(message)
  if (check) {
    return { kind: 'key', key: CHECK_CONSTRAINT[check[1]] ?? 'error.invalidValue' }
  }

  const unique = /violates unique constraint "([^"]+)"/.exec(message)
  if (unique) {
    return { kind: 'key', key: UNIQUE_CONSTRAINT[unique[1]] ?? 'error.duplicate' }
  }

  if (/invalid input syntax for type uuid/.test(message)) {
    return { kind: 'key', key: 'error.badId' }
  }
  if (/numeric field overflow/.test(message)) {
    // The message names no field — QA #20 — so neither can this. Saying which
    // kind of value went wrong is the honest limit.
    return { kind: 'key', key: 'error.numericOverflow' }
  }
  if (/violates row-level security policy/.test(message)) {
    // §9: a write RLS refused means the UI rendered a control it should not
    // have. A bug to fix — and a sentence the user still needs meanwhile.
    return { kind: 'key', key: 'error.notAllowed' }
  }
  if (/violates not-null constraint/.test(message)) {
    return { kind: 'key', key: 'error.missingValue' }
  }
  if (/violates foreign key constraint/.test(message)) {
    return { kind: 'key', key: 'error.badReference' }
  }

  // A JS exception, whether thrown or already stringified. Never copy.
  if (JS_EXCEPTION.test(message) || isRuntimeException(error)) {
    return { kind: 'key', key: 'error.unexpected' }
  }

  // §9's exception, added for Swahili: a message the app can name EXACTLY is
  // read from the bundle with the values it carried. Anything else, including
  // a near miss, is still shown as written.
  const named = translateDbMessage(message)
  if (named) return { kind: 'key', key: named.key, values: named.values }

  return { kind: 'verbatim', message }
}

/**
 * A thrown JS runtime error, as opposed to an `Error` the app constructed to
 * carry a Postgres message — which is how every hook in this codebase
 * surfaces one.
 */
function isRuntimeException(error: unknown): boolean {
  return (
    error instanceof TypeError ||
    error instanceof ReferenceError ||
    error instanceof SyntaxError ||
    error instanceof RangeError
  )
}
