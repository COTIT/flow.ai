# Ruaha 360 — database schema

Status: **applied and in service** on the cloud dev project since
10 September 2026; migration history recorded 11 September 2026.
Authority: Ruaha 360 Overview Plan v2. Target: MVP demo, 30 September 2026.

## Apply order

```bash
pnpm db:list        # migration history: files vs database
pnpm db:push:dry    # what would apply
pnpm db:push        # apply
pnpm db:rls         # the 193 policy assertions
```

**Cloud dev project only** — there is no local Supabase and none is wanted.
Details, and the `--db-url` route the CLI needs here, are in
`supabase/README.md`.

A schema change is a **new migration**. History stores each migration's
statements, so editing an existing file desyncs it from the database silently.

Never edit schema in the dashboard. Migrations in the repo are the single
source of truth, and they are what makes a later self-host a copy rather
than a rewrite.

## Files

| file | contents |
|---|---|
| `..._foundations.sql` | enums, `hectares` domain, `audit_log`, `write_audit()`, `set_updated_at()`, the provenance convention |
| `..._geography.sql` | `country` `project` `village` |
| `..._identity.sql` | `person` `household` `household_member` `app_user` `membership`, all RLS helpers |
| `..._production.sql` | `crop` `farm` `farm_manager` `plot` `crop_cycle` `harvest_report` |
| `..._pue.sql` | `equipment_category` `equipment` `pue_request` + status guard |
| `..._energy.sql` | `village_capacity` `energy_estimate` + recompute trigger |
| `..._market.sql` | `buyer` `buyer_demand` `opportunity` `opportunity_supply` + commitment guard |
| `..._views.sql` | seven Control Tower views, all `security_invoker` |
| `20260929090001_farmer_login.sql` | `app_user.must_change_password`, `login_issue`, phone normalisation, phone + GPS required at registration, `app_farmer_login_issue`, `app_password_changed` |
| `20260929090002_household_four_eyes.sql` | `app_verify` refuses a household to the officer who registered it |
| `20260929090003_surveys.sql` | `survey` `survey_question` `survey_response` `survey_answer` `survey_voucher` `voucher_event`, the survey and redeem RPCs, the audit timeline, three staff views |
| `20261005090001_village_create.sql` | `village` / `village_capacity` writes scoped to `app_manages_project`; `app_create_village` (village + first capacity row) |
| `20261005090002_register_many_cycles.sql` | `app_register_farmer` takes `cycles[]`, one per crop on the plot; the single `cycle` payload is still accepted |
| `20261005090003_plot_documents.sql` | private bucket `plot-documents`, `plot_document` (staff only, append-only), storage policies by village folder |
| `20261005090004_resource_catalogue.sql` | `equipment.can_rent` / `can_buy` / `indicative_rent_per_day`, `pue_request.acquisition` (`acquisition_mode`), `loan_product` (listings only, no finance terms), pipeline view splits buy value from rent |
| `seed.sql` | labelled demo data, two villages, six accounts |
| `seed_surveys.sql` | run after `seed.sql`: a second Ilundo officer, household verifiers, five demo surveys, three vouchers |
| `tests/rls_test.sql` | 193 assertions on the policies, including `tests/mvp_security_test.sql` and `tests/survey_test.sql` |

## Conventions

**Every table has a uuid primary key.** Including the join tables —
`household_member`, `farm_manager`, `opportunity_supply` — which carry an `id`
plus a `unique` constraint on the natural pair rather than a composite primary
key. `write_audit()` records `audit_log.record_id` as a uuid, so a
composite-keyed table cannot be audited at all; the surrogate key is what
makes audit coverage uniform instead of a per-table exception.

All three join tables are audited. `opportunity_supply` in particular is the
traceability claim the product makes — who committed whose harvest to which
buyer, and when — which is precisely the record research item I is about.

**Config vs observed.** Config tables (`country` `project` `village` `crop`
`equipment_category` `equipment` `buyer` `village_capacity`) carry no
provenance. Observed tables carry the full block:

```
source · captured_at · captured_by · verification · verified_by · verified_at
confidence · evidence_ref · created_at · updated_at · deleted_at
```

**One source per row.** Where two facts need different provenance, they are
different rows — which is why `harvest_report` is a series rather than two
columns on `crop_cycle`.

**`village_id` is denormalised** down `farm → plot → crop_cycle →
harvest_report`, held true by composite foreign keys. Every policy is one
indexed comparison; drift is structurally impossible.

**No DELETE policies anywhere.** Removal is `deleted_at`. Membership uses
`revoked_at`. Access ends; the audit trail does not.

**`(select app_villages())`, never `app_villages()`** inside a policy — the
subquery form is evaluated once per statement rather than once per row.

**Units.** Area is always stored in hectares (`hectares` domain). `area_unit`
is a display preference. Money is `numeric(14,2)` plus a currency code,
default TZS, and every price is indicative.

## Roles

Three product roles plus `admin` for operations.

| | farmer | field_officer | ops | admin |
|---|---|---|---|---|
| scope | own person + household + farms | assigned village(s) | whole project | whole project |
| person, farm, plot, cycle | read own | read + write in village | read + write | read + write |
| pue_request | create + edit own draft | create + assist in village | **review and decide** | review and decide |
| energy_estimate | read own | read in village | read | read |
| buyer, buyer_demand | none | none | read + write | read + write |
| opportunity | read if own supply is in it | read in village | read + write | read + write |
| Tower views | empty | village rows | project rows | project rows |
| membership | read own | read own | grant farmer + officer | grant any role |
| app login | sign in with phone | issue / reset in village | issue / reset | issue / reset |
| survey | read live in project | read live | read all | **author and publish** |
| survey response | answer once per household (RPC) | read in village | read | read |
| voucher | read own, code via RPC | redeem in village (RPC) | redeem, void, reconcile | redeem, void, reconcile |

## Where the security actually lives

RLS is row-level. Three things it cannot express are done with triggers:

- **`pue_request_guard`** — legal status transitions, reviewer-only review,
  server-stamped `decided_by`/`decided_at`, and content frozen once a request
  leaves draft. Without it, `pue_update_own` lets a farmer approve their own
  request.
- **`opportunity_supply_guard`** — the same harvested kilos cannot be promised
  to two live buyers (research item I). `opportunity_resum` keeps
  `offered_quantity_kg` equal to the sum of the attached supply lines, on both
  sides of a move.
- **`pue_recompute_estimate`** — clients hold no write policy on
  `energy_estimate` at all. The trigger is the only writer.

- **`survey_guard`** — admin-only authoring, `draft → live → closed`,
  content frozen once live, publish checks, server-stamped `published_by`.
- **`survey_block_reason` / `voucher_block_reason`** — the survey
  eligibility and redeem rules, shared by the list and the write so they
  cannot disagree. See business-rules §16–17.
- **`voucher_event_append_only`** — the audit trail cannot be edited or
  deleted (cleanup opts in with `ruaha.cleanup = on`).

**The one ownership exception.** `app_farmer_login_issue` and
`app_password_changed` are owned by `postgres`, not `ruaha_observed_writer`:
they must write `auth.users`, and the writer role has no `auth` access by
design. `postgres` bypasses RLS, so both check scope explicitly, and
`tests/survey_test.sql` asserts it.

RLS helper functions are `SECURITY DEFINER` because they read `membership`;
`membership`'s own read policy is kept trivial (`user_id = auth.uid()`) so
nothing recurses.

**No policy may join back through a policied table.** Two tables that each
reference the other inside a policy make Postgres abort the query with
`infinite recursion detected in policy for relation ...` — it is not a slow
query or a wrong answer, it is a hard error. `person` → `household_member` →
`household` → `household_member` was exactly that shape. Any cross-table
question a policy needs answered is therefore resolved in a `SECURITY DEFINER`
helper, which reads with RLS off and so cannot cycle:

| helper | answers |
|---|---|
| `app_households()` | households the caller's person belongs to |
| `app_household_persons()` | persons sharing a household with the caller |
| `app_staff_households()` | households in the caller's villages, **if staff** |
| `app_supplied_opportunities()` | opportunities whose supply traces to the caller's farms |
| `app_staff_opportunities()` | opportunities in the caller's villages, **if staff** |

The staff guard lives *inside* `app_staff_households()`, not at the call site.
`app_villages()` returns villages for every role holding a membership row —
farmers included — so a village-scoped helper that omitted `app_is_staff()`
would quietly hand a farmer every household in their village while still
satisfying the RLS test suite.

## Distinctions the schema enforces

- planned capacity vs measured — `capacity_basis` has **no** `'measured'` value
- prospective vs approved demand — `v_village_energy` splits by request status
- peak vs sum — `simultaneity_factor` is applied, raw sums also exposed
- expected vs actual harvest — `harvest_kind`, one current row each
- opportunity vs sale — no delivery, payment or contract tables exist
- cycle area vs land area — the column is named `cycle_area_ha` because
  intercropping means cycle areas can exceed the village's hectares

## Open — carried from Plan v2 S22

- confidence scale (`low/medium/high` chosen provisionally)
- farmer identifiers; PII kept deliberately minimal until this closes
- season / harvest-window taxonomy (`season_label` is free text)
- grades, lots, collection points (`quality_note` is free text)
- meter provider — no meter tables exist, on purpose
- finance terms, deposit, rate, schedule — absent; research item C
  (BoT Tier 2 classification) is unresolved

## Not in this schema, deliberately

Wallets and QR payment rails · crowdfarming and investor ROI · full commodity
exchange · end-to-end logistics and export traceability · meter fleet
management · offline sync queues · pgvector and any AI surface.

Reference screens from Control Center and African Farmers Market show several
of these. A screen existing does not put it in scope.

A survey voucher is **not** a payment instrument: it is a single-use claim on
a fixed cash incentive, redeemed face to face at the office. There is no
balance, no transfer and no wallet (decision of 29 September 2026).
