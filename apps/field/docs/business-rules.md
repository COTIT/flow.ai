# Ruaha 360 — business rules and data access

Status: **specification, nothing built.** Migrations have never been applied.
Companions: `schema.md`, `screens-and-components.md`, `supabase/migrations/`.
Authority: Ruaha 360 Overview Plan v2.

This document says **how each function behaves**. Where a rule is enforced by
the database, that is stated — the client must not duplicate it, and must not
work around it.

---

## 0 · Rules of engagement

1. **The database is the authority on truth.** Status transitions, provenance,
   estimates, double-commit limits and access are enforced by triggers and RLS.
   The client's job is to offer only legal actions and to display errors
   honestly.
2. **Never compute in the client what the database computes.** Listed in §11.
3. **Never pre-validate a rule the database enforces** in order to show a
   friendlier message. Call it, catch the error, show it. The pre-check will
   drift from the trigger within a fortnight.
4. **Zero rows is an answer.** RLS returning nothing means "you may not see
   this". Render an empty state. Do not retry, do not error, do not escalate.
5. **Every observed-table insert carries provenance.** There is one helper for
   this and it is not optional (§4).

---

## 1 · Registration — `app_register_farmer(payload jsonb)`

Migration `..._rpc.sql`, rewritten by `20260925090001_mvp_security` and
`20261005090002_register_many_cycles`. `SECURITY DEFINER`, owned by the
`NOLOGIN`, `NOBYPASSRLS` role `ruaha_observed_writer`, so RLS still applies
inside: an officer can only register into a village they are assigned to.

### Why an RPC and not five inserts

The officer is on a phone with intermittent signal. Five chained client
inserts that fail on the third leave a person and household with no farm, and
no way for the officer to know. One transaction either fully succeeds or
changes nothing.

### Payload

```jsonc
{
  "client_ref": "uuid",          // REQUIRED. Generated once per form session.
  "village_id": "uuid",
  "person":    { "given_name": "", "family_name": "", "phone": "", "confidence": "high" },
  "household": { "label": "", "is_head": true },          // or { "existing_household_id": "uuid" }
  "farm":      { "label": "", "latitude": null, "longitude": null },  // or { "existing_farm_id": "uuid" }
  "plot":      { "label": "", "area_ha": "1.8" },
  "cycles": [                    // one per crop grown on the plot
    { "crop_id": "uuid", "season_label": "", "area_ha": "1.6",
      "tree_count": null, "unit_count": null,
      "planted_on": "2026-03-05", "harvest_start": "2026-09-01",
      "harvest_end": "2026-09-30", "status": "growing",
      "harvest": { "quantity_kg": "4100", "confidence": "medium", "reported_for": "2026-09-15" } }
  ]
}
```

`cycles` and each cycle's `harvest` are optional. `person`, `plot` and a farm
are not. A plot carries several concurrent cycles (intercropping), so planted
area across them can exceed the plot's area. The same crop twice on one plot
is refused (`each crop may be registered once per plot`); one crop's missing
measure refuses the whole registration.

Before 5 Oct 2026 the payload took a single `cycle` plus `harvest`. That shape
is still accepted, as a one-element `cycles`, so a draft typed on an older
build submits unchanged.

### Returns

```json
{ "person_id":"…", "household_id":"…", "farm_id":"…", "plot_id":"…",
  "village_id":"…",
  "crop_cycle_ids":["…"], "harvest_report_ids":["…"],
  "crop_cycle_id":"…", "harvest_report_id":"…",   // the first of each, for older clients
  "replayed": false }
```

### Idempotency — `client_ref`

**The client generates one UUID when the form is opened** and sends the same
value on every retry. A replay returns the original result with
`"replayed": true` and creates nothing. Without this, a request that times out
after the transaction committed produces a duplicate farmer on retry — the
single most likely data-integrity failure in field conditions.

Store `client_ref` in the IndexedDB draft alongside the form values. Do not
regenerate it on retry. Do not regenerate it on page reload.

### Provenance it sets

| record | source |
|---|---|
| person, household, farm, plot, crop_cycle | `field_verified` |
| harvest_report (expected) | `farmer_reported` |

`captured_by = auth.uid()`, `captured_at = now()`, `verification = 'unverified'`
on everything. **Registration never verifies.** Verification is a separate,
deliberate act (§5).

The expected harvest is `farmer_reported` even though an officer typed it,
because the *farmer* is the source of the number. This distinction is the
whole point of S13 and the UI must reflect it.

### Validation performed in the RPC

- `client_ref` present, else refuse
- caller is staff, else refuse
- **phone present** — it is the farmer's login name (§15). Normalised by
  `app_normalize_phone` to `+255` and nine digits; anything that is not a
  Tanzanian mobile is refused
- **farm GPS present** for a new farm — the form fills and locks latitude and
  longitude from the handset and opens them for typing only when GPS fails
- crop measure matches `crop.measured_by` — an `area` crop requires `area_ha`,
  a `tree_count` crop requires `tree_count`, a `unit_count` crop requires
  `unit_count`. **The client must branch the same way** (`MeasureInput`), but
  the RPC is the enforcement.

### Client contract

- One submit button. Disabled while in flight.
- On network failure: keep the draft, show **not yet submitted**, offer retry
  with the same `client_ref`.
- On success: clear the draft, navigate to `/officer/people/$personId`.
- Never show a success state for a write that only reached IndexedDB.

---

## 2 · PUE request status machine

Enforced by `pue_request_guard` (migration `..._pue.sql`).

### Legal transitions

```
draft         → submitted | withdrawn
submitted     → under_review | withdrawn
under_review  → approved | rejected
```

Everything else raises. `approved`, `rejected` and `withdrawn` are terminal.

### Who may do what

| state | farmer (own) | field officer | ops / admin |
|---|---|---|---|
| draft | edit, submit, withdraw | edit, submit (assisting) | edit, submit |
| submitted | withdraw only | withdraw only | start review, withdraw |
| under_review | nothing | nothing | approve, reject |
| approved / rejected / withdrawn | nothing | nothing | nothing |

Moving to `under_review`, `approved` or `rejected` requires `ops` or `admin`.
The trigger raises `only ops or admin may review a request` otherwise.

### Server-stamped fields

`submitted_at`, `decided_at` and `decided_by` are set by the trigger. **The
client must never send them.** If it does, the trigger overwrites them.

### Content freeze

Once a request leaves `draft`, a non-reviewer changing `equipment_id`,
`quantity`, `hours_per_day`, `days_per_week` or `purpose` raises
`a submitted request cannot be edited`. The farmer UI must not render those
inputs as editable after submission.

### Insert rule

A request may only be **created** as `draft` or `submitted`. Creating one
directly as `approved` raises. This also applies to seeds and fixtures.

---

## 3 · Energy estimate

Enforced by `pue_recompute_estimate` (migration `..._energy.sql`).

### The rule

`energy_estimate` has **no client write policy**. The trigger is the only
writer. Any attempt to insert or update it fails.

The trigger fires on insert, and on update of `equipment_id`, `quantity`,
`hours_per_day`, `days_per_week`, `village_id` or `confidence`.

### The arithmetic

```
est_power_kw     = rated_power_kw × quantity
est_kwh_per_day  = rated_power_kw × quantity × hours_per_day
est_kwh_per_week = rated_power_kw × quantity × hours_per_day × days_per_week
```

These are `GENERATED ALWAYS … STORED` columns, so the stored result can never
disagree with the stored inputs.

### Snapshotting

The trigger copies `rated_power_kw` from the catalogue **at compute time**. If
ops later changes the catalogue, existing estimates keep the figure they were
computed from. This is deliberate: an estimate must stay explainable after the
inputs beneath it move.

Fallbacks: `hours_per_day` and `days_per_week` fall back to the catalogue's
`typical_*`, then to `0`. An equipment row with a null `rated_power_kw` causes
the estimate to be **deleted**, not zeroed.

### Client contract

- `EnergyEstimatePanel` may recompute locally **for preview only**, using the
  same three formulas, before submit.
- After any write, the displayed figure comes from the `energy_estimate` row.
  Never from a local calculation.
- If a local preview and the stored row ever differ, the component is wrong.

---

## 4 · Provenance

### The helper — write it on day one

```ts
withProvenance(payload, source: SourceType)
// injects: source, captured_at = now(), captured_by = session.app_user.id
```

Every insert into `person`, `household`, `farm`, `plot`, `crop_cycle`,
`harvest_report`, `pue_request` goes through it. Retrofitting provenance into
thirty call sites later is a bad week.

### The database is authoritative (migration `20260925090001_mvp_security`)

The helper keeps call sites honest; it is not the security boundary.

- `person`, `household`, `farm`, `plot`, `crop_cycle`, `harvest_report`,
  `household_member`, `farm_manager` and `registration_receipt` accept **no
  direct client writes**. They change only through `app_register_farmer`,
  `app_update_observed_record`, `app_verify` and `app_supersede_harvest`.
  Those RPCs run as `ruaha_observed_writer` — `NOLOGIN`, `NOBYPASSRLS`, owns
  no table — so village-scoped RLS still applies inside them.
- Registration-created rows carry server provenance (`field_verified`,
  `captured_by` = the officer); the expected harvest stays `farmer_reported`.
- `pue_request` and `buyer_demand` inserts are re-stamped by the
  `stamp_captured_input` trigger: `captured_by = auth.uid()`,
  `captured_at = now()`, `verification = 'unverified'`, and for requests
  `source = 'farmer_reported'`. Updates cannot change any of them. A forged
  value is replaced, not rejected.
- Relationship rows (`household_member`, `farm_manager`) require both
  endpoints in the same village. A cross-village join fails with
  `new row violates row-level security policy`.

Config tables — `country`, `project`, `village`, `crop`, `equipment_category`,
`equipment`, `buyer`, `village_capacity` — have **no** provenance columns.
Do not try to set them.

### Which source, where

| written by | source |
|---|---|
| officer registering or editing a record | `field_verified` |
| a number the farmer supplied, typed by anyone | `farmer_reported` |
| a farmer editing their own profile | **not in MVP** — farmers are read-only on person and production records |
| a PUE request | `farmer_reported` (schema default) |
| an energy estimate | `model_estimated` (trigger sets it) |
| meter data | `sensor_derived` — **not in MVP, nothing writes this** |
| a recorded transaction | `transaction_derived` — **not in MVP** |

`buyer_demand` deliberately has **no** `source` column. The five categories
classify how a fact about the *productive economy* was learned. A buyer's
stated requirement is a counterparty input; it carries `captured_by` and
`verification` instead.

---

## 5 · Verification — `app_verify(table, id)`

One entry point, so `verification` can never be set without a verifier.

- Allowed tables: `person`, `household`, `farm`, `plot`, `crop_cycle`,
  `harvest_report`. Anything else raises.
- Caller must be staff.
- Sets `verification = 'verified'`, `verified_by = auth.uid()`,
  `verified_at = now()`.

### Rules

- **Nobody verifies their own farmer-reported figure by accident.** The UI
  shows verification as a distinct, deliberate action with the verifier's name
  attached, never a checkbox inside an edit form.
- Registration never verifies (§1).
- **Four eyes on households.** The officer who registered a household may
  not verify it: `a household must be verified by someone other than the
  officer who registered it`. A household is what a survey incentive is paid
  to (§16), so one person must never create one and vouch for it alone. The
  verify queue does not offer the button on your own registrations.
- `disputed` and `pending` exist in the enum; verification may move
  `unverified` or `pending → verified`, while `disputed` rows require
  correction before verification. Do not build a separate dispute flow.
- After verifying, invalidate the record's query key **and** the farmer-facing
  key — a farmer's My Farm badge must change without a manual refresh.

---

## 5a · Officer corrections — `app_update_observed_record(...)`

Officers can correct the editable fields on a person, household, farm, plot,
or crop-cycle detail section. The UI sends only the allowlisted fields; IDs,
relationships, village scope, provenance, and verification columns remain
server-controlled.

- The caller must be field staff and the row must be visible in an assigned
  village through RLS.
- Required text, GPS ranges, numeric ranges, crop measures, and date windows
  are validated again by Postgres. Database errors are shown verbatim.
- A successful correction stamps `source = 'field_verified'`,
  `captured_by = auth.uid()`, and `captured_at = now()`.
- A successful correction resets verification to `unverified` and clears
  `verified_by` / `verified_at`; the officer must verify the corrected record
  again.
- Harvest reports are corrected only through `app_supersede_harvest(...)`.
  The old row remains auditable and the replacement is the single current row.

The officer detail pages expose section-level Edit / Save / Cancel controls.
Save failure leaves the entered values in place and keeps the form actionable.
There is no unverify action: verification is deliberately one-way.

---

## 6 · Harvest reports — `app_supersede_harvest(...)`

A harvest figure is a **series**, not a value. Expected estimates get revised
through a season; the old numbers stay auditable.

### The invariant

`harvest_one_current` — a partial unique index — permits exactly one row per
`(crop_cycle_id, kind)` where `is_current and deleted_at is null`. This is the
double-counting guard that research item I asks for, enforced by the database.

### The procedure

`app_supersede_harvest(cycle, kind, quantity_kg, source, confidence, reported_for)`
retires the current row and inserts the replacement in one transaction, so the
index never sees two current rows.

**Never** write `harvest_report` directly from the client for a revision. A
naive insert violates the index; a naive update loses the history.

### Expected vs actual

- `expected` — usually `farmer_reported`, low or medium confidence
- `actual` — usually `field_verified` after harvest
- They are independent series. A cycle may have one current row of each.
- Aggregation reads only `is_current` rows. `v_harvest_available` filters to
  `kind = 'expected'`.

---

## 7 · Aggregation

All of it lives in views (migration `..._views.sql`). The client does not
aggregate.

### Current-row selection
Only `is_current and deleted_at is null` harvest reports participate. A
superseded estimate never reaches a total.

### Committed supply
`v_harvest_available.available_kg = quantity_kg − committed_kg`, where
`committed_kg` sums `opportunity_supply` rows on opportunities with status
`proposed`, `shared` or `accepted`. Declined and lapsed opportunities release
their supply.

### Simultaneity
Village peak is **not** the sum of rated power. `v_village_energy` multiplies
by `village_capacity.simultaneity_factor` and exposes both the raw sum and the
corrected peak. The UI shows the factor next to the peak it was applied to.

### Prospective vs approved
Never summed into one "demand" figure.

- prospective = requests in `submitted` or `under_review` — an **application**
- approved = requests in `approved` — still **not measured consumption**
- capacity = `capacity_kw` with its `basis`, which is `planned` or `nameplate`
  and never `measured`

### Area
`v_village_production.cycle_area_ha` is the sum of **cycle** areas. Intercropping
means several cycles share one plot, so this can exceed the village's land.
Label it "planted area across cycles". Never "land area", never "hectares
farmed".

---

## 8 · Opportunities

### What an opportunity is not
Not a sale, not a delivery, not a payment, not a contract. `accepted` means
both sides agreed to keep talking. The UI states this on every opportunity
screen.

### Attaching supply
`opportunity_supply` rows link an opportunity to specific `harvest_report`
rows. This foreign key **is** the product's traceability claim — "every
headline traces back to records" is enforced structurally, not reported.

### The double-commit guard
`opportunity_supply_guard` raises if the same harvest report is committed
beyond its available quantity across live opportunities:

```
over-commitment: 4100.00 kg available, 4100.00 kg already committed, 100.00 kg requested
```

Show that message to the user as written. Do not pre-check in the client.

### Headline consistency
`opportunity.offered_quantity_kg` is re-summed by `opportunity_resum` on every
supply change. The client never sets it — the column is not in the client's
insert or update grant. The tile and the drill-down cannot drift apart.

### Who may write, and the status machine
Only project-level ops or admin (`app_manage_village`) may insert or update an
opportunity or attach supply; officers keep read access. `opportunity_guard`
enforces, in Postgres:

```
insert            → proposed only
proposed → shared | declined | lapsed
shared   → accepted | declined | lapsed
accepted → declined | lapsed
declined, lapsed  → terminal
```

`opportunity_supply_guard` locks the opportunity then the harvest row
(`for update`), so two concurrent attachments to one harvest serialise and the
second sees the first's commitment. It also requires a current, undeleted
expected harvest in the opportunity's village, crop and cycle.

`v_demand_match.committed_kg` is the server-side sum of commitments on the
matching current harvests, once per demand and village over overlapping
harvest windows. The client does not add it up.

---

## 9 · Error contract

Postgres error messages in this schema are written to be read by humans.
Surface them verbatim in a toast or inline alert.

| raised by | message | UI |
|---|---|---|
| `pue_request_guard` | `illegal transition x -> y` | bug — the UI offered an illegal action. Log it. |
| `pue_request_guard` | `only ops or admin may review a request` | bug — control should not have been rendered |
| `pue_request_guard` | `a submitted request cannot be edited` | show inline on the form |
| `opportunity_supply_guard` | `over-commitment: …` | show verbatim; it names the numbers |
| `app_register_farmer` | `this crop is measured by area: area_ha is required` | show on the field |
| `app_register_farmer` | `client_ref is required …` | bug — fix the client |
| RLS | *no error, zero rows* | empty state |
| unique violation `harvest_one_current` | constraint error | you wrote a harvest row directly — use `app_supersede_harvest` |
| `app_register_farmer` | `a phone number is required so the farmer can sign in` · `the farm location is required: …` | show on the form |
| `app_verify` | `a household must be verified by someone other than the officer who registered it` | bug — the queue should not have offered it |
| `app_farmer_login_issue` | `this phone number already has an app login` · `add a phone number before issuing a login` | show verbatim on the login card |
| `app_password_changed` | `choose a new password first` | show verbatim |
| `survey_guard` | `only an admin may author surveys` · `a live survey cannot be edited` · `a survey needs at least one question before it goes live` · `every choice question needs at least two options` | show verbatim; the first is a bug |
| `app_survey_submit` | the eligibility reason (§16) · `question N is required` · `question N: choose one of the listed options` | show verbatim on the survey |
| `app_voucher_redeem` | `you registered this household, …` · `you verified this household, …` · `this voucher is held for an audit: …` · `voucher already redeemed on … by …` · `this voucher expired on …` · `record which ID document you checked` | show verbatim on the redeem screen |
| unique violation `survey_response_one_per_household` | constraint error | the household already answered — a race the RPC normally catches first |

Three of these mean the client rendered a control it should not have. Treat
them as bugs to fix, not conditions to handle politely.

---

## 10 · Data access conventions

### Query keys

```
['session']
['villages']
['person', personId]            ['people', villageId, filters]
['farm', farmId]                ['farms', villageId]
['cycle', cycleId]              ['harvest', cycleId]
['equipment', projectId]        ['equipmentItem', equipmentId]
['requests', { villageId, status }]   ['request', requestId]
['estimate', requestId]
['demand', demandId]            ['demands', projectId]
['opportunity', opportunityId]
['tower', 'production', villageId]
['tower', 'energy', villageId]
['tower', 'market', villageId]
['tower', 'quality', villageId]
['surveyEligibility']           ['survey', surveyId]
['farmerVouchers']              ['voucherCode', voucherId]
['voucherTimeline', voucherId]
['surveyAdmin']                 ['surveyAdmin', surveyId]
['surveyVouchers', surveyId]    ['surveyTally', surveyId]
['redemptionLog', from, to]     ['loginHistory', personId]
```

### Invalidation map

| after | invalidate |
|---|---|
| `app_register_farmer` | `['people', village]` `['farms', village]` `['tower', *, village]` |
| `app_verify` | the record key, plus `['tower','quality',village]` and the farmer-facing key |
| `app_update_observed_record` | the edited record, `['people', village]`, related farm/cycle/harvest keys, `['verifyQueue']`, officer home, Tower quality/production, and farmer-facing records |
| request insert / update | `['requests', …]` `['request', id]` `['estimate', id]` `['tower','energy',village]` |
| approve / reject | as above, plus the farmer's `['request', id]` |
| `app_supersede_harvest` | `['harvest', cycle]` `['tower','production',village]` `['tower','market',village]` |
| `opportunity_supply` change | `['opportunity', id]` `['demand', demandId]` `['tower','market',village]` |
| `app_farmer_login_issue` | `['loginHistory', person]` |
| `app_survey_submit` | `['surveyEligibility']` `['farmerVouchers']` `['voucherTimeline', id]` |
| `app_create_village` | `['villageCapacity']` `['villages']` `['tower']` |
| `plot_document` upload | `['plotDocuments', plot]` |
| `equipment` insert | `['equipment', 'mine']` |
| `loan_product` insert | `['loanProducts']` |
| survey write / publish / close | `['surveyAdmin']` `['surveyAdmin', id]` `['surveyEligibility']` |
| `app_voucher_redeem` · `app_voucher_void` | `['voucherTimeline', id]` `['surveyVouchers', survey]` `['surveyAdmin']` `['redemptionLog']` |

### Reads
Views for anything aggregate. Tables for record detail. Nested `select()` for
graph reads — one round trip, RLS applies all the way down:

```ts
supabase.from('farm').select(`
  id, label, verification,
  plot ( id, label, area_ha,
         crop_cycle ( id, crop_id, harvest_start, harvest_end,
                      harvest_report ( quantity_kg, kind, is_current, source ) ) )
`)
```

---

## 11 · Never compute in the client

- energy estimates that will be stored (preview only, §3)
- village aggregates of any kind
- available vs committed supply
- coverage percentages
- whether a status transition is permitted *as a substitute* for calling it
- who may see a row

The client may compute: display-unit conversion (ha ↔ acre), formatting,
sorting a page of already-fetched rows, and the live estimate preview.

---

## 12 · Drafts and interrupted saves

- Every multi-field form persists to IndexedDB on change. Registration keys by
  form name plus `client_ref` (`useDraft`); the equipment request, buyer
  create, demand create, supply attach and officer correction forms use
  `usePersistentForm`, keyed `form:<user>:<scope>:<form>` so another account on
  the same handset never sees the draft.
- Restore on mount, before submit is enabled. Clear only after a confirmed
  server write. Local saves are serialised per key, so a save still in flight
  cannot resurrect a draft that `finish()` cleared.
- Create forms send their draft's `clientRef` as the row id. After an
  uncertain response, `recoverInsert` looks that id up: a row that exists is
  the success; otherwise the original error is shown verbatim.
- A storage failure is shown (`draft.storageError`), never swallowed.
- A draft renders an `UnsavedDraftBadge`. **An unsaved write must look
  unsaved.** No success toast for something that only reached local storage.
- Retry is explicit and user-initiated. No background queue, no replay engine,
  no conflict resolution. That is offline sync, and Plan v2 defers it until
  field testing proves it necessary.

---

## 13 · Session and access

- `useSession()` resolves: auth user → `app_user` → memberships
  (`revoked_at is null`).
- `app_user` is readable only by its own account, and writable only in
  `locale` and `display_name`. Provenance names for other actors come from
  `app_actor_names(ids)`, which returns a name only for actors on records the
  caller can already see; zero rows means omit the name, never show the id.
- Active membership determines the landing route and the nav. Held in memory
  and the URL, never in the token.
- **Route guards are UX.** A farmer who hand-types `/ops/requests` gets the
  page shell and zero rows. That is correct behaviour, not a hole.
- A revoked membership takes effect on the next query. No session
  invalidation is required, because RLS re-evaluates every statement.

---

## 14 · Units, formats, time

| | rule |
|---|---|
| area | stored in hectares (`hectares` domain). Display per `country.default_area_unit`. Convert at the edge only |
| weight | kg, 2 dp |
| power | kW, 3 dp |
| energy | kWh, 3 dp |
| money | `numeric(14,2)` + explicit currency code. Default TZS. Prices always labelled **indicative**. A survey incentive is a fixed amount, not a price: never "indicative", never "earnings", "wallet" or "balance" |
| percentages | 1 dp |
| timestamps | `timestamptz`, stored UTC, displayed `Africa/Dar_es_Salaam` |
| harvest windows, planting dates | plain `date`. No timezone. Never converted |
| alignment | `tabular-nums` wherever figures stack |

---

## 15 · Farmer logins — `app_farmer_login_issue(person)`

Farmers sign in with the **phone number their officer registered** and a
temporary password. No email, no SMS, no third party.

- After registration the officer's screen shows a login card: the phone and
  an 8-character temporary password. **Shown once.** It exists in plain text
  only in the RPC's return value — never stored, never in a draft or a URL.
- The login is created by the database: an `auth.users` row under a hidden
  address derived from the phone (`255…@farmers.ruaha360.test`), `app_user`
  linked to the person, and a farmer membership in the person's village. The
  sign-in screen maps any spelling of the phone to that address
  (`src/lib/phone.ts` mirrors `app_normalize_phone`).
- One login per person (`app_user_person_unique`) and one login per phone.
  Family members who share a phone share a household anyway (§16).
- **First sign-in forces a new password.** `must_change_password` sends
  every guard to `/set-password`. After `supabase.auth.updateUser`, the
  client calls `app_password_changed`, which refuses while the stored hash is
  still the temporary one — the flag cannot be cleared without a real change.
- **While the flag is set, the database refuses survey answers.** The
  officer who read out the temporary password cannot answer as the farmer.
- A forgotten password is reset the same way: the officer issues a new
  temporary one from Person detail. The farmer is signed out everywhere.
  Every issue and reset is in `login_issue`, with the officer's name.

---

## 16 · Surveys and incentives

Admin authors a survey; a farmer answers once per household and receives a
single-use voucher for a **fixed cash incentive, paid at the Ruaha office**.
An incentive is not a wage, a balance or a payment instrument.

### Authoring — admin only (`survey_guard`)
- `draft → live → closed`. A draft is edited freely; publishing checks there
  is at least one question, two options per choice question, an English label
  per option, and a future closing date. Publishing stamps `published_by` and
  `published_at` (`clock_timestamp()`).
- **Once live, nothing changes** — not the questions, not the incentive. The
  amount is also copied onto each voucher at issue.
- Ops sees every survey and its results; only admin authors or publishes.

### Who may answer — `survey_block_reason`, shared by the list and the submit
In order; the first reason that applies is the answer:
1. the survey is live and not past its closing date
2. the caller is a farmer linked to a person, **not on a temporary password**
3. the person belongs to **exactly one** household
4. the survey covers the household's village
5. **the household has not answered** (`survey_response_one_per_household`)
6. the household is verified, **by someone other than its registrar** (§5)
7. the household **existed before the survey went live** — registering a
   household to catch an open survey does not work
8. the survey's household cap (`max_households`) is not reached

The farmer's list shows the reason verbatim. The client never re-derives it.

### Answering — `app_survey_submit(survey, answers, client_ref)`
- Answers are keyed by question id and validated in the RPC (required,
  option values, numbers, text length).
- `client_ref` makes a retry replay the same voucher.
- One transaction: response, answers, voucher (10-character Crockford code,
  30-day expiry, `audit_required = random() < audit_rate`) and an `issued`
  event.

### Notification
In-app only: a count on the Surveys tab of surveys the household may answer
now. The MVP sends no SMS and no push.

---

## 17 · Redeeming a voucher — three staff, a random audit, a named trail

- **The code is the household's.** Staff cannot read `code` (column grants);
  the household reads it through `app_voucher_code`. A voucher can only be
  redeemed when its code is shown at the office — as a QR code, or typed.
- **Every scan is logged** (`app_voucher_lookup` writes `scanned`). A real
  code scanned outside the officer's villages answers "not found" and is
  logged as `refused` — ops sees the attempt.
- **Three different staff.** Whoever registered or verified the household
  cannot redeem its vouchers; nor can a member of the household. With §5's
  four-eyes rule, paying a fake household needs three colluding staff.
- **Random audit hold.** A share of vouchers (`survey.audit_rate`, default
  15%) is marked at issue. Only ops or admin redeem those, face to face. The
  flag is invisible to farmers and field officers until the scan.
- **ID sighted.** The redeemer records which ID document they checked (NIDA
  card, voter card, driving licence, village letter) and confirms the name
  matches. The type is stored; **never the number, never a photo**.
- Row lock plus `status = 'issued'`: two officers scanning at once cannot both
  pay. The same officer retrying is a replay, not a second payout. Expired and
  void vouchers are refused. Ops may void an unredeemed voucher, with a reason.
- **The audit trail.** `voucher_event` is append-only; actor name and role
  are snapshots. `app_voucher_timeline` merges it with the household's
  registration and verification, the farmer's login history, the survey's
  publication and the answer — filtered by who asks (farmer: issued,
  answered, redeemed, voided; field staff: plus scans; ops/admin: everything).
- Cash reconciliation: `app_redemption_totals` gives redemptions per officer
  per day; `app_redemption_log` lists each one.

**The honest limit.** Without an outside identity source the app cannot
prove two households are not one family. These rules make that fraud need
three staff, catch it at random, and put a name on every step.

---

## 18 · Open

- Confidence is `low/medium/high` provisionally (S22)
- Season taxonomy — `season_label` is free text (S22)
- Grades and quality — `quality_note` is free text (S12 later)
- Finance terms, deposit, rate, schedule — **absent by design**; research item
  C (BoT Tier 2 classification) is unresolved and nothing here models lending
- Meter ingestion — no tables, no `sensor_derived` writer. Provider unselected
  (S22)

## 19 · Plot title documents — staff only (added 5 Oct 2026)

The one exception to "no photo upload", approved by the product owner on
5 Oct 2026. Migration `20261005090003_plot_documents`.

- **What:** photos or PDFs of a plot's title deed or other land papers, taken
  at registration so whoever verifies the plot can check its fields against
  them. Demo images only in this build.
- **Where:** private bucket `plot-documents`, path
  `{village_id}/{plot_id}/{id}-{file name}`, 10 MB a file, JPEG / PNG / WebP /
  HEIC / PDF. A row in `plot_document` describes each file.
- **Who:** field officers, ops and admin of the plot's village read and add
  (`app_staff_villages()`), on both the rows and the files. **Farmers see
  nothing**, including their own plot's documents.
- **Append-only:** no update or delete for any client. A wrong upload is
  followed by a right one; the record of what was shown stays.
  `uploaded_by` is the caller, never what the client sends.
- **Order:** the plot exists only after `app_register_farmer` returns, so the
  register screen keeps picked files in a separate IndexedDB draft
  (`register-docs:{client_ref}`) and files them against the returned
  `plot_id`. A retry lands on the same path; "already exists" counts as filed.
  The draft is cleared only once every file is filed.
- **Not validated in the client:** size and type are the bucket's rules; a
  refused file shows storage's own message.

## 20 · Resource catalogue — rent, buy, loan listings (added 5 Oct 2026)

Migration `20261005090004_resource_catalogue`; demo rows in
`supabase/seed_resources.sql`.

- **Equipment** is offered to rent, to buy, or both (`can_rent`, `can_buy`,
  `check (can_rent or can_buy)`), with an indicative price to buy and/or an
  indicative rent per day. Both are guides, never quotations.
- **A request** records `acquisition` (`rent` | `buy`, default `buy`, so every
  request before this change is a purchase). `pue_request_acquisition_check`
  raises `this equipment is not offered for rent` (or `for sale`).
  `pue_request_guard` locks `acquisition` with the other fields once a
  request is submitted. The energy estimate does not depend on it.
- **`v_village_pue_pipeline`:** `indicative_value` is price × quantity for
  BUY requests only; rent requests add `indicative_rent_per_day` × quantity in
  their own column. A rent is never counted as a purchase price, and neither
  figure is financed value or a loan book.
- **Loan listings** (`loan_product`): code, names, descriptions and an
  indicative range (`check (min > 0 and min <= max)`). **Listing only.** No
  interest, deposit, term, schedule or repayment column exists or may be added
  while research item C (BoT Tier 2 classification) is open. A farmer cannot
  request a loan in the app; they ask at the office.
- **Who writes:** ops and admin of the project (`app_manages_project`), by
  direct insert — config tables, no provenance. No delete: a row is retired
  with `is_active = false`.

