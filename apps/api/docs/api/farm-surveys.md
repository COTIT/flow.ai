# Surveys and vouchers

An admin authors a survey. A farmer answers it once per household and receives a single-use voucher for a fixed cash incentive, paid face to face at the office. A voucher is not a payment instrument: no balance, no transfer. Code: `src/modules/farm_surveys/`, `src/modules/farm_vouchers/`, migration `farm_surveys`.

Money is always `{minor, currency, decimals, currency_id}`: integer minor units of a workspace currency. TZS 10,000.00 at 2 decimals is `minor: 1000000`. Every total is summed in the database as `bigint`.

## Authoring (`/v1/workspace/{workspace}/farm/surveys`)

Only a farm **admin** authors: `POST /surveys`, `PATCH /surveys/{survey}`, and `POST`, `PUT` or `DELETE` on `/surveys/{survey}/questions[/{question}]`. ops and officers get `403`.

| Survey field | Rule |
|---|---|
| `title_en` | required, ≤ 200 |
| `title_sw`, `description_en`, `description_sw` | optional |
| `reward_minor` | required integer > 0 |
| `currency_id` | required, a currency of this workspace |
| `village_id` | optional; null = every village |
| `max_households` | optional cap on answering households |
| `audit_percent` | 0–100, default 15: share of vouchers held for an ops audit |
| `closes_at` | optional RFC 3339 time |

Questions have `position`, `kind` (`single_choice`, `multi_choice`, `number`, `text`, `yes_no`), `prompt_en`, `prompt_sw`, `required` (default true) and, for choice kinds, `options [{value, label_en, label_sw}]`.

- **Status** moves `draft → live → closed` only (`POST /surveys/{survey}/publish`, `/close`). A closed survey never reopens.
- **Publishing** checks that the survey has a question, that every choice question has two or more options, that every option has a value and an English label, and that the closing date is in the future. It stamps `published_by` and `published_at`.
- **Frozen once live.** Edits get `a live survey cannot be edited`; question changes get `questions can only change while the survey is a draft`. A database trigger enforces both, and the status machine too.

**Who sees surveys:** ops and admin see all of them. Everyone else sees live and closed surveys that are for every village or for one of their own; drafts are `404` to them.

## Answering

`GET /surveys/eligibility` lists every live or closed survey with `{survey_id, eligible, reason, response_id, voucher_id}`. The reason is the same text a submit would refuse with. The first reason that applies wins:

1. `survey not found` / `this survey is closed`
2. `your account is not linked to a registered farmer`
3. `set your own password before answering surveys` (still on the temporary password)
4. `your household record needs checking by an officer` (not exactly one household)
5. `this survey is not open in your village`
6. `your household has already answered this survey`
7. `your household has not been verified yet`
8. `your household must be verified by a second staff member`
9. `this survey opened before your household was registered`
10. `this survey has reached its limit`

`POST /surveys/{survey}/responses {answers: {<question id>: value}, client_ref}`:

- **Answer checks:**
  - `question N is required`
  - `question N: choose one of the listed options` (single choice) / `choose from the listed options` (multi choice)
  - `question N: enter a number` (numbers as numbers or numeric text)
  - `question N: answer in at most 2000 characters`
  - `question N: answer yes or no`
  - `an answer does not match any question in this survey`
- **One transaction:** the survey row is locked so the cap can't race. It writes the response (`farmer_reported`, captured by the farmer), the answers, the voucher and an `issued` event:
  - code: 10 Crockford characters;
  - expiry: 30 days;
  - amount: copied from the survey;
  - `audit_required`: drawn at random with `audit_percent`.
- **Replies:** `201 {response_id, voucher_id, code, amount, expires_at, replayed: false}`.
  - The same `client_ref` again: `200` with the first result and `replayed: true`.
  - Someone else's `client_ref`: `this answer reference belongs to someone else`.
  - The household answers once (`farm_survey_responses_one_per_household`).

`GET /surveys/{survey}/results` is for staff: response count, issued/redeemed/outstanding `{count, minor}`, expired and void counts, a per-option tally, and number summaries. `GET /surveys/{survey}/vouchers` is for ops and admin and has **no codes**.

## Vouchers

- **`GET /vouchers/mine`**: the household's vouchers **with their codes**. This is the only place a code is ever shown. The audit flag is not shown. A voided voucher carries `void_reason` (the reason ops gave); it is `null` otherwise.
- **`POST /vouchers/lookup {code}`**: field staff only (`403` otherwise).
  - Case, spaces and dashes don't matter; O reads as 0, I and L as 1, and a `PREFIX:` from a QR code is dropped.
  - Every lookup is logged:
    - a code in the caller's villages writes a `scanned` event;
    - a real code elsewhere answers `found: false` and writes a `refused` event, which only ops see.
  - **At most 30 lookups a minute per account**, unknown codes included, then `429`.
  - The reply shows status, amount, the household and respondent, `needs_ops`, `can_redeem` and `blocked_reason`.
- **`POST /vouchers/redeem {code, id_type, name_confirmed}`**: field staff of the voucher's village (`404` elsewhere). Refusals, in order:
  - `voucher already redeemed on DD Mon YYYY HH:MI by NAME` (Tanzanian time)
  - `this voucher was voided: REASON`
  - `this voucher expired on DD Mon YYYY`
  - `you registered this household, so another staff member must redeem this voucher`
  - `you verified this household, so another staff member must redeem this voucher`
  - `you cannot redeem a voucher for your own household`
  - `this voucher is held for an audit: ops or admin must redeem it in person`
  - then `record which ID document you checked` (`nida`, `voter`, `driving_licence`, `village_letter`: the type only, never a number) and `confirm that the name on the ID matches the household`.

  The voucher row is locked: parallel redeems pay once and write one `redeemed` event. The same officer retrying gets their redemption back with `replayed: true`.
- **`POST /vouchers/{voucher}/void {reason}`**: ops and admin only (`only ops or admin may void a voucher`). The voucher must be unredeemed and a reason is required. Writes a `voided` event.
- **`GET /vouchers/{voucher}/timeline`**: one ordered trail, filtered by who asks:
  - the household sees `answered`, `issued`, `redeemed`, `voided` (with the reason);
  - field staff of the village also see household registration and verification, the survey's publication and scans, but no refusals or audit flags;
  - ops and admin see everything, including refused scans and the respondent's login issues.

  `farm_voucher_events` is append-only, with the actor's name and role as they were at the time.

## Cash reconciliation (ops and admin)

- `GET /redemptions?from=YYYY-MM-DD&to=YYYY-MM-DD`: each redemption on those Tanzanian calendar days, with the officer, village, household, survey, amount and ID type.
- `GET /redemptions/totals?from&to`: per day, officer and currency, `{vouchers, minor}` summed as `bigint`.
