import { localiseDbDate } from '@/lib/format'

/**
 * The schema's own messages, in the user's language.
 *
 * business-rules §9 says these messages are written to be read by humans and
 * are shown verbatim, and `errors.ts` holds that line: an unfamiliar sentence
 * is far more likely to be one of ours than machine noise. That is still the
 * default. This module adds the other half for a user who does not read
 * English: a message the app can name EXACTLY is shown from the i18n bundle,
 * carrying every value it had (a question number, a date, a name), and
 * anything else falls through to verbatim.
 *
 * Each rule records `sql`, the literal it was written against.
 * `dbMessages.test.ts` fails if that literal leaves the migrations, so a
 * reworded message cannot stop matching in silence and drop a Swahili farmer
 * back to English.
 *
 * Scope: the farmer surface, and the officer's registration, verification,
 * login-card and voucher-redemption messages, from the 29 Sep migrations.
 * Everything else is still verbatim.
 */
export interface DbMessageRule {
  /** i18n key the message resolves to. */
  key: string
  /** The literal in the migrations this rule matches. */
  sql: string
  /** Anchored: the whole message has to match. */
  pattern: RegExp
  /** Interpolation values, from the pattern's capture groups. */
  values?: (match: RegExpExecArray) => Record<string, string>
  /** A real message this rule matches, for the tests. */
  example: string
}

export interface TranslatedDbMessage {
  key: string
  values: Record<string, string>
}

/** A fixed sentence: exact text, no values. */
function fixed(key: string, text: string, sql = text): DbMessageRule {
  return {
    key,
    sql,
    pattern: new RegExp(`^${text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`),
    example: text,
  }
}

/** "question N …": the position is what tells a farmer which answer to fix. */
function question(key: string, tail: string, sql: string): DbMessageRule {
  return {
    key,
    sql,
    pattern: new RegExp(`^question (\\d+)${tail.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`),
    values: (m) => ({ position: m[1] }),
    example: `question 7${tail}`,
  }
}

export const DB_MESSAGE_RULES: DbMessageRule[] = [
  // ── survey eligibility (survey_block_reason) ──
  fixed('dbError.surveyNotFound', 'survey not found'),
  fixed('dbError.surveyClosed', 'this survey is closed'),
  fixed('dbError.notLinkedToFarmer', 'your account is not linked to a registered farmer'),
  fixed('dbError.setOwnPasswordFirst', 'set your own password before answering surveys'),
  fixed('dbError.householdNeedsChecking', 'your household record needs checking by an officer'),
  fixed('dbError.surveyNotInVillage', 'this survey is not open in your village'),
  fixed('error.surveyAlreadyAnswered', 'your household has already answered this survey'),
  fixed('dbError.householdNotVerified', 'your household has not been verified yet'),
  fixed(
    'dbError.householdNeedsSecondVerifier',
    'your household must be verified by a second staff member',
  ),
  fixed(
    'dbError.surveyOpenedBeforeRegistration',
    'this survey opened before your household was registered',
  ),
  fixed('dbError.surveyLimitReached', 'this survey has reached its limit'),

  // ── survey submission (app_survey_submit) ──
  fixed('dbError.signInFirst', 'sign in first'),
  question('dbError.questionRequired', ' is required', 'question % is required'),
  question('dbError.questionChooseOne', ': choose one of the listed options', 'question %: choose one of the listed options'),
  question('dbError.questionChooseSome', ': choose from the listed options', 'question %: choose from the listed options'),
  question('dbError.questionEnterNumber', ': enter a number', 'question %: enter a number'),
  question('dbError.questionTooLong', ': answer in at most 2000 characters', 'question %: answer in at most 2000 characters'),
  question('dbError.questionYesNo', ': answer yes or no', 'question %: answer yes or no'),
  // Developer-facing: a retry guard and a shape check. A farmer cannot act on
  // them, so they read as the generic error.
  fixed('error.unexpected', 'client_ref is required so a retry cannot answer twice'),
  fixed('error.unexpected', 'answers must be an object keyed by question'),
  fixed('error.unexpected', 'this answer reference belongs to someone else'),
  fixed('error.unexpected', 'an answer does not match any question in this survey'),

  // ── password (app_password_changed guard) ──
  fixed('dbError.choosePasswordFirst', 'choose a new password first'),

  // ── voucher redemption (voucher_block_reason, app_voucher_redeem) ──
  {
    // The database names nobody when it has no name to give, and says so in
    // English. That fallback must not be printed inside a Swahili sentence.
    key: 'dbError.voucherAlreadyRedeemedByOther',
    sql: "'voucher already redeemed on %s by %s'",
    pattern: /^voucher already redeemed on (.+) by another staff member$/,
    values: (m) => ({ date: localiseDbDate(m[1]) }),
    example: 'voucher already redeemed on 05 Sep 2026 14:30 by another staff member',
  },
  {
    key: 'dbError.voucherAlreadyRedeemed',
    sql: "'voucher already redeemed on %s by %s'",
    pattern: /^voucher already redeemed on (.+?) by (.+)$/,
    values: (m) => ({ date: localiseDbDate(m[1]), name: m[2] }),
    example: 'voucher already redeemed on 05 Sep 2026 14:30 by Salima Officer',
  },
  {
    key: 'dbError.voucherVoided',
    sql: "'this voucher was voided: '",
    pattern: /^this voucher was voided: ([\s\S]*)$/,
    values: (m) => ({ reason: m[1] }),
    example: 'this voucher was voided: issued twice',
  },
  {
    key: 'dbError.voucherExpired',
    sql: "'this voucher expired on %s'",
    pattern: /^this voucher expired on (.+)$/,
    values: (m) => ({ date: localiseDbDate(m[1]) }),
    example: 'this voucher expired on 03 Mar 2026',
  },
  fixed(
    'dbError.redeemOwnRegistration',
    'you registered this household, so another staff member must redeem this voucher',
  ),
  fixed(
    'dbError.redeemOwnVerification',
    'you verified this household, so another staff member must redeem this voucher',
  ),
  fixed('dbError.redeemOwnHousehold', 'you cannot redeem a voucher for your own household'),
  fixed(
    'dbError.voucherHeldForAudit',
    'this voucher is held for an audit: ops or admin must redeem it in person',
  ),
  fixed('dbError.voucherNotFound', 'voucher not found'),
  fixed('dbError.recordIdDocument', 'record which ID document you checked'),
  fixed('dbError.confirmIdName', 'confirm that the name on the ID matches the household'),
  fixed('error.notAllowed', 'only staff may look up vouchers'),
  fixed('error.notAllowed', 'only staff may redeem vouchers'),

  // ── household verification (four eyes) ──
  fixed(
    'dbError.verifyOtherOfficer',
    'a household must be verified by someone other than the officer who registered it',
  ),
  fixed('dbError.recordNotFound', 'record not found or outside your villages'),
  fixed('dbError.notAwaitingVerification', 'record is not awaiting verification'),
  fixed('error.notAllowed', 'only field staff may verify records'),

  // ── registration and the farmer login card ──
  fixed(
    'dbError.phoneFormat',
    'phone number must be a Tanzanian mobile number, for example +255 712 345 678',
  ),
  fixed('dbError.phoneRequired', 'a phone number is required so the farmer can sign in'),
  fixed(
    'dbError.farmLocationRequired',
    'the farm location is required: capture the GPS position before saving',
  ),
  fixed('dbError.cropNeedsArea', 'this crop is measured by area: area_ha is required'),
  fixed('dbError.cropNeedsTrees', 'this crop is measured by tree count: tree_count is required'),
  fixed('dbError.cropNeedsUnits', 'this crop is measured by unit count: unit_count is required'),
  fixed('dbError.personNotFound', 'person not found or outside your villages'),
  fixed('dbError.addPhoneFirst', 'add a phone number before issuing a login'),
  fixed(
    'dbError.staffPasswordNotHere',
    'this person signs in as staff: their password cannot be reset here',
  ),
  fixed('dbError.phoneHasLogin', 'this phone number already has an app login'),
  fixed('error.notAllowed', 'only field staff may register a farmer'),
  fixed('error.notAllowed', 'only field staff may issue a farmer login'),
  // Authored by the app, not the database (src/app/session.ts). Its own guard
  // test lives with the session code, so `sql` names that file's literal.
  {
    key: 'error.accountUnreadable',
    sql: 'Your account could not be read. Try again.',
    pattern: /^Your account could not be read\. Try again\.$/,
    example: 'Your account could not be read. Try again.',
  },
  // A missing field the form should have supplied, not something to fix.
  fixed('error.unexpected', 'village_id is required'),
  fixed('error.unexpected', 'unknown crop'),
]

/**
 * A database reason shown as DATA — `survey_block_reason` and
 * `voucher_block_reason` return their English sentence in a row, not as an
 * error. One the app names exactly is read from the bundle; any other is shown
 * as written, with its first letter raised (§9).
 */
export function dbReasonText(
  t: (key: string, values?: Record<string, string>) => string,
  reason: string,
): string {
  const named = translateDbMessage(reason)
  return named ? t(named.key, named.values) : reason.charAt(0).toUpperCase() + reason.slice(1)
}

/**
 * The message as an i18n key plus its values, or `null` when the app does not
 * know it exactly and it should be shown as written.
 */
export function translateDbMessage(message: string): TranslatedDbMessage | null {
  const text = message.trim()
  for (const rule of DB_MESSAGE_RULES) {
    const match = rule.pattern.exec(text)
    if (match) return { key: rule.key, values: rule.values?.(match) ?? {} }
  }
  return null
}
