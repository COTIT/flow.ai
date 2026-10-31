# RUAHA assertions → genesy tests

Every assertion of RUAHA's database suite (`/Users/cotit/Documents/RUAHA/supabase/tests/*.sql`, run by `pnpm db:rls`) and where genesy proves the same rule, or why it does not apply. `tests/farm_mapping.rs` keeps this file honest: exactly 280 rows, every named test exists, every N/A has a reason.

**Count.** RUAHA has **280** assertions (`pg_temp.assert_eq` + `pg_temp.assert_raises` calls). The "282" in the brief counted the two helper definitions and the two `drop function` lines in `rls_test.sql`; RUAHA's own `CLAUDE.md` says 280. Ported: **194**. N/A: **86**.

**How rules map.** RUAHA enforces access with Postgres RLS and `SECURITY DEFINER` RPCs; "zero rows" means "not yours". genesy enforces it in the API: the farm guard (`src/http/farm.rs`), village-scoped SQL in each repo, and composite `(workspace_id, village_id)` foreign keys that make cross-village links impossible. A RUAHA "zero rows" is a genesy `404` (or an empty list); a RUAHA "raises" is a `403`/`422` with RUAHA's message where it has one. Several assertions share one test.

**Beyond RUAHA** (rules genesy adds or changes, with their tests):
- Joining a household that already has a head: RUAHA's `ON CONFLICT DO NOTHING` silently dropped the new member; genesy makes them a member (`is_head` false) and refuses a second head — `tests/farm_registration.rs::joining_a_household_with_a_head_does_not_drop_the_new_member`.
- Harvest revisions accept only `farmer_reported` or `field_verified` as source (RUAHA accepted all five). RUAHA's app only ever wrote `field_verified` (`useOfficerEdit.ts`) and no assertion uses another; the other three are machine sources nothing writes yet — `tests/farm_records.rs::a_harvest_revision_keeps_one_current_row_and_the_history`.
- Cross-workspace denial on every route group, farm-only users shut out of the rest of the workspace, phone and GPS only for village staff, voucher lookup rate limit, redeem under a 5-way race, temporary passwords never logged — see `tests/farm_*.rs`.

| # | RUAHA assertion | What it checks | genesy test, or why not |
|---|---|---|---|
| 1 | rls_test.sql #1 | farmer Neema sees exactly her own farm | `tests/farm_records.rs::a_farmer_reads_her_own_farm_and_household_only` |
| 2 | rls_test.sql #2 | farmer Neema cannot see the Kimaro farm | `tests/farm_records.rs::a_farmer_reads_her_own_farm_and_household_only` |
| 3 | rls_test.sql #3 | farmer Neema sees herself plus her household only | `tests/farm_records.rs::a_farmer_reads_her_own_farm_and_household_only` |
| 4 | rls_test.sql #4 | Ilundo officer sees all six Ilundo persons | `tests/farm_records.rs::officers_read_their_village_only` |
| 5 | rls_test.sql #5 | Ilundo officer sees no Mgama persons | `tests/farm_records.rs::officers_read_their_village_only` |
| 6 | rls_test.sql #6 | Mgama officer sees both Mgama persons | `tests/farm_records.rs::officers_read_their_village_only` |
| 7 | rls_test.sql #7 | Mgama officer sees only the Mgama farm | `tests/farm_records.rs::officers_read_their_village_only` |
| 8 | rls_test.sql #8 | farmer cannot approve their own request | N/A because PUE requests and energy estimates are out of scope (G14) |
| 9 | rls_test.sql #9 | farmer cannot move their own request to review | N/A because PUE requests and energy estimates are out of scope (G14) |
| 10 | rls_test.sql #10 | ops cannot mint an admin membership | `tests/farm_tenancy.rs::ops_cannot_mint_admin_or_self_elevate` |
| 11 | rls_test.sql #11 | ops cannot self-elevate | `tests/farm_tenancy.rs::ops_cannot_mint_admin_or_self_elevate` |
| 12 | rls_test.sql #12 | ops cannot flip a farmer membership to admin | `tests/farm_tenancy.rs::ops_cannot_touch_admin_memberships` |
| 13 | rls_test.sql #13 | farmer sees no buyer demand | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 14 | rls_test.sql #14 | farmer sees no buyers | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 15 | rls_test.sql #15 | Tower energy view is empty for a farmer | N/A because energy, village capacity and Control Tower energy views are out of scope (G14) |
| 16 | rls_test.sql #16 | Tower production view is empty for a farmer | N/A because Control Tower aggregate views are not ported in this phase; farmers get 403/404 on every staff-only read |
| 17 | rls_test.sql #17 | farmer DOES see the opportunity her own supply is inside | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 18 | rls_test.sql #18 | nobody can hand-write an energy estimate | N/A because energy, village capacity and Control Tower energy views are out of scope (G14) |
| 19 | rls_test.sql #19 | already-committed harvest cannot be promised to a second live opportunity | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 20 | rls_test.sql #20 | both villages appear in the Tower (each has a current capacity row) | N/A because energy, village capacity and Control Tower energy views are out of scope (G14) |
| 21 | rls_test.sql #21 | Ilundo approved peak = (15.0 + 2 x 1.5) x 0.600 = 10.800 kW | N/A because energy, village capacity and Control Tower energy views are out of scope (G14) |
| 22 | rls_test.sql #22 | Ilundo September maize expected = 12000 kg (superseded estimate excluded) | `tests/farm_records.rs::a_harvest_revision_keeps_one_current_row_and_the_history` |
| 23 | rls_test.sql #23 | available = 12000 - 6400 already committed | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 24 | rls_test.sql #24 | farmer cannot verify a record | `tests/farm_records.rs::verification_stamps_the_verifier_and_is_one_way` |
| 25 | rls_test.sql #25 | farmer cannot use the officer correction RPC | `tests/farm_records.rs::correction_rules` |
| 26 | rls_test.sql #26 | farmer cannot supersede a harvest figure | `tests/farm_records.rs::a_harvest_revision_keeps_one_current_row_and_the_history` |
| 27 | rls_test.sql #27 | verification rejects invalid tables | `tests/farm_records.rs::verification_stamps_the_verifier_and_is_one_way` |
| 28 | rls_test.sql #28 | verification rejects already verified rows | `tests/farm_records.rs::verification_stamps_the_verifier_and_is_one_way` |
| 29 | rls_test.sql #29 | verification rejects an out-of-village row | `tests/farm_records.rs::verification_stamps_the_verifier_and_is_one_way` |
| 30 | rls_test.sql #30 | verification rejects a missing row | `tests/farm_records.rs::verification_stamps_the_verifier_and_is_one_way` |
| 31 | rls_test.sql #31 | verification stamps the officer and timestamp | `tests/farm_records.rs::verification_stamps_the_verifier_and_is_one_way` |
| 32 | rls_test.sql #32 | correction rejects server-controlled fields | `tests/farm_records.rs::correction_rules` |
| 33 | rls_test.sql #33 | correction rejects blank required text | `tests/farm_records.rs::correction_rules` |
| 34 | rls_test.sql #34 | correction rejects invalid GPS | `tests/farm_records.rs::correction_rules` |
| 35 | rls_test.sql #35 | correction rejects an out-of-village row | `tests/farm_records.rs::correction_rules` |
| 36 | rls_test.sql #36 | correction resets verification and stamps provenance | `tests/farm_records.rs::a_correction_resets_verification_and_stamps_provenance` |
| 37 | rls_test.sql #37 | harvest correction leaves exactly one current row | `tests/farm_records.rs::a_harvest_revision_keeps_one_current_row_and_the_history` |
| 38 | rls_test.sql #38 | harvest correction preserves the old row | `tests/farm_records.rs::a_harvest_revision_keeps_one_current_row_and_the_history` |
| 39 | rls_test.sql #39 | exactly one current expected figure survives per cycle | `tests/farm_records.rs::a_harvest_revision_keeps_one_current_row_and_the_history` |
| 40 | mvp_security_test.sql #1 | RPC writer cannot login or bypass RLS | N/A because genesy has no client database access: there is no writer role to guard; every write is a service call behind the farm guard |
| 41 | mvp_security_test.sql #2 | RPC writer owns no tables | N/A because genesy has no client database access: there is no writer role to guard; every write is a service call behind the farm guard |
| 42 | mvp_security_test.sql #3 | client cannot assume writer | N/A because genesy has no client database access: there is no writer role to guard; every write is a service call behind the farm guard |
| 43 | mvp_security_test.sql #4 | full account profile is self-only | `tests/farm_records.rs::a_farm_only_user_cannot_read_workspace_users` |
| 44 | mvp_security_test.sql #5 | identity reassignment denied | `tests/farm_records.rs::correction_rules` |
| 45 | mvp_security_test.sql #6 | farmer verification spoof denied | `tests/farm_records.rs::correction_rules` |
| 46 | mvp_security_test.sql #7 | farmer provenance spoof denied | `tests/farm_records.rs::correction_rules` |
| 47 | mvp_security_test.sql #8 | safe self preferences remain editable | N/A because account preferences (locale, display name) are the users module, unchanged here; phone-only accounts get 403 there by design (G11) |
| 48 | mvp_security_test.sql #9 | officer cannot read buyer order book | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 49 | mvp_security_test.sql #10 | officer cannot bypass verification RPC | `tests/farm_records.rs::correction_rules` |
| 50 | mvp_security_test.sql #11 | direct relationship injection denied | `tests/farm_registration.rs::a_registration_cannot_join_another_villages_household_or_farm` |
| 51 | mvp_security_test.sql #12 | officer supply write denied | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 52 | mvp_security_test.sql #13 | officer opportunity update affects no rows | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 53 | mvp_security_test.sql #14 | RPC registration remains usable | `tests/farm_registration.rs::registration_creates_the_chain_with_server_provenance` |
| 54 | mvp_security_test.sql #15 | registration replay creates no duplicate | `tests/farm_registration.rs::a_replay_returns_the_original_and_creates_nothing` |
| 55 | mvp_security_test.sql #16 | out-of-village registration denied | `tests/farm_registration.rs::registration_is_refused_outside_the_officers_villages` |
| 56 | mvp_security_test.sql #17 | offered total is server-only | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 57 | mvp_security_test.sql #18 | demand view owns committed total | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 58 | mvp_security_test.sql #19 | demand capture cannot be forged | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 59 | mvp_security_test.sql #20 | cannot accept before sharing | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 60 | mvp_security_test.sql #21 | terminal opportunity cannot reopen | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 61 | mvp_security_test.sql #22 | farmer request provenance is normalised, not trusted | N/A because PUE requests and energy estimates are out of scope (G14) |
| 62 | mvp_security_test.sql #23 | farmer resolves the officer on her own records | `tests/farm_records.rs::names_of_actors_resolve_only_on_visible_records` |
| 63 | mvp_security_test.sql #24 | farmer cannot resolve unrelated account names | `tests/farm_records.rs::names_of_actors_resolve_only_on_visible_records` |
| 64 | mvp_security_test.sql #25 | farmer opportunity write denied | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 65 | mvp_security_test.sql #26 | anonymous registration denied | `tests/farm_registration.rs::farmers_and_anonymous_callers_cannot_register` |
| 66 | mvp_security_test.sql #27 | anonymous verification denied | `tests/farm_tenancy.rs::outsiders_get_404_and_anonymous_401` |
| 67 | mvp_security_test.sql #28 | anonymous name lookup denied | `tests/farm_records.rs::another_workspace_reads_nothing` |
| 68 | mvp_security_test.sql #29 | officer cannot join another village's household | `tests/farm_registration.rs::a_registration_cannot_join_another_villages_household_or_farm` |
| 69 | mvp_security_test.sql #30 | cross-village household join denied for project staff | `tests/farm_registration.rs::a_registration_cannot_join_another_villages_household_or_farm` |
| 70 | mvp_security_test.sql #31 | cross-village farm join denied for project staff | `tests/farm_registration.rs::a_registration_cannot_join_another_villages_household_or_farm` |
| 71 | mvp_security_test.sql #32 | a denied registration leaves nothing behind | `tests/farm_registration.rs::crop_rules_refuse_the_whole_registration` |
| 72 | mvp_security_test.sql #33 | multi-month demand counts Ilundo availability once | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 73 | mvp_security_test.sql #34 | multi-month demand counts Ilundo commitments once | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 74 | mvp_security_test.sql #35 | one row per village, never per harvest | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 75 | mvp_security_test.sql #36 | ops attaches supply and the database sums it | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 76 | mvp_security_test.sql #37 | supply from another village is refused | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 77 | mvp_security_test.sql #38 | an opportunity cannot be born shared | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 78 | mvp_security_test.sql #39 | lapsed is terminal | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 79 | mvp_security_test.sql #40 | Mgama officer reads no Ilundo farm managers | `tests/farm_records.rs::farm_managers_are_read_within_scope` |
| 80 | mvp_security_test.sql #41 | Mgama officer reads exactly Mgama farm managers | `tests/farm_records.rs::farm_managers_are_read_within_scope` |
| 81 | mvp_security_test.sql #42 | Ilundo officer still reads Ilundo farm managers | `tests/farm_records.rs::farm_managers_are_read_within_scope` |
| 82 | mvp_security_test.sql #43 | Ilundo officer reads no Mgama farm managers | `tests/farm_records.rs::farm_managers_are_read_within_scope` |
| 83 | mvp_security_test.sql #44 | project-wide ops still reads every seeded farm manager | `tests/farm_records.rs::farm_managers_are_read_within_scope` |
| 84 | mvp_security_test.sql #45 | farmer still reads her own farm manager | `tests/farm_records.rs::farm_managers_are_read_within_scope` |
| 85 | mvp_security_test.sql #46 | mixed membership: no staff read of other Ilundo household members | `tests/farm_records.rs::mixed_membership_gives_staff_reads_only_where_staff` |
| 86 | mvp_security_test.sql #47 | mixed membership: own household still read as a farmer | `tests/farm_records.rs::mixed_membership_gives_staff_reads_only_where_staff` |
| 87 | mvp_security_test.sql #48 | mixed membership: Mgama household members read as staff | `tests/farm_records.rs::mixed_membership_gives_staff_reads_only_where_staff` |
| 88 | mvp_security_test.sql #49 | mixed membership: no staff read of Ilundo opportunity supply | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 89 | mvp_security_test.sql #50 | mixed membership: Mgama opportunity supply read as staff | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 90 | mvp_security_test.sql #51 | mixed membership: no staff read of other Ilundo farm managers | `tests/farm_records.rs::mixed_membership_gives_staff_reads_only_where_staff` |
| 91 | mvp_security_test.sql #52 | mixed membership: own household farm manager still read | `tests/farm_records.rs::mixed_membership_gives_staff_reads_only_where_staff` |
| 92 | mvp_security_test.sql #53 | mixed membership: Mgama farm manager read as staff | `tests/farm_records.rs::mixed_membership_gives_staff_reads_only_where_staff` |
| 93 | survey_test.sql #1 | officer issues a first login from the registered phone | `tests/farm_logins.rs::an_officer_issues_a_first_login_from_the_registered_phone` |
| 94 | survey_test.sql #2 | a new login must change its temporary password | `tests/farm_logins.rs::an_officer_issues_a_first_login_from_the_registered_phone` |
| 95 | survey_test.sql #3 | the new login is a farmer in her own village | `tests/farm_logins.rs::an_officer_issues_a_first_login_from_the_registered_phone` |
| 96 | survey_test.sql #4 | the login name is the normalised phone | `tests/farm_logins.rs::an_officer_issues_a_first_login_from_the_registered_phone` |
| 97 | survey_test.sql #5 | issuing again resets the password instead of creating a second login | `tests/farm_logins.rs::issuing_again_resets_instead_of_creating_a_second_login` |
| 98 | survey_test.sql #6 | still exactly one login for the person | `tests/farm_logins.rs::issuing_again_resets_instead_of_creating_a_second_login` |
| 99 | survey_test.sql #7 | every issue is recorded with who issued it | `tests/farm_logins.rs::issuing_again_resets_instead_of_creating_a_second_login` |
| 100 | survey_test.sql #8 | a phone number already used by another login is refused | `tests/farm_logins.rs::issue_refusals` |
| 101 | survey_test.sql #9 | no login without a phone number | `tests/farm_logins.rs::issue_refusals` |
| 102 | survey_test.sql #10 | clients cannot read login history | `tests/farm_logins.rs::who_may_issue_and_read_the_history` |
| 103 | survey_test.sql #11 | the officer sees the login history, with names | `tests/farm_logins.rs::who_may_issue_and_read_the_history` |
| 104 | survey_test.sql #12 | a Mgama officer cannot issue an Ilundo login | `tests/farm_logins.rs::who_may_issue_and_read_the_history` |
| 105 | survey_test.sql #13 | a Mgama officer sees no Ilundo login history | `tests/farm_logins.rs::who_may_issue_and_read_the_history` |
| 106 | survey_test.sql #14 | a farmer cannot issue logins | `tests/farm_logins.rs::who_may_issue_and_read_the_history` |
| 107 | survey_test.sql #15 | a farmer cannot clear the temporary-password flag | `tests/farm_logins.rs::the_temporary_password_does_not_count_as_a_new_one` |
| 108 | survey_test.sql #16 | anonymous login issue denied | `tests/farm_logins.rs::who_may_issue_and_read_the_history` |
| 109 | survey_test.sql #17 | the temporary password does not count as a new one | `tests/farm_logins.rs::the_temporary_password_does_not_count_as_a_new_one` |
| 110 | survey_test.sql #18 | a changed password clears the flag | `tests/farm_logins.rs::the_temporary_password_does_not_count_as_a_new_one` |
| 111 | survey_test.sql #19 | registration needs a phone number | `tests/farm_registration.rs::registration_input_rules` |
| 112 | survey_test.sql #20 | registration needs a Tanzanian mobile number | `tests/farm_registration.rs::registration_input_rules` |
| 113 | survey_test.sql #21 | registration needs the farm GPS position | `tests/farm_registration.rs::registration_input_rules` |
| 114 | survey_test.sql #22 | registration stores the normalised phone | `tests/farm_registration.rs::registration_creates_the_chain_with_server_provenance` |
| 115 | survey_test.sql #23 | the registering officer cannot verify the household | `tests/farm_records.rs::a_household_needs_a_second_pair_of_eyes` |
| 116 | survey_test.sql #24 | the registering officer may still verify the person | `tests/farm_records.rs::a_household_needs_a_second_pair_of_eyes` |
| 117 | survey_test.sql #25 | a second staff member verifies the household | `tests/farm_records.rs::a_household_needs_a_second_pair_of_eyes` |
| 118 | survey_test.sql #26 | ops cannot author surveys | `tests/farm_surveys.rs::only_an_admin_authors_and_publishing_checks_the_content` |
| 119 | survey_test.sql #27 | a survey cannot go live without questions | `tests/farm_surveys.rs::only_an_admin_authors_and_publishing_checks_the_content` |
| 120 | survey_test.sql #28 | a choice question needs at least two options | `tests/farm_surveys.rs::only_an_admin_authors_and_publishing_checks_the_content` |
| 121 | survey_test.sql #29 | publishing stamps who and when | `tests/farm_surveys.rs::only_an_admin_authors_and_publishing_checks_the_content` |
| 122 | survey_test.sql #30 | a live survey cannot be edited | `tests/farm_surveys.rs::a_live_survey_is_frozen_and_a_closed_one_stays_closed` |
| 123 | survey_test.sql #31 | no questions are added after publishing | `tests/farm_surveys.rs::a_live_survey_is_frozen_and_a_closed_one_stays_closed` |
| 124 | survey_test.sql #32 | a closed survey cannot reopen | `tests/farm_surveys.rs::a_live_survey_is_frozen_and_a_closed_one_stays_closed` |
| 125 | survey_test.sql #33 | farmers do not see draft surveys | `tests/farm_surveys.rs::farmers_see_live_surveys_never_drafts` |
| 126 | survey_test.sql #34 | farmers see live surveys in their project | `tests/farm_surveys.rs::farmers_see_live_surveys_never_drafts` |
| 127 | survey_test.sql #35 | a verified household may answer | `tests/farm_surveys.rs::a_verified_household_answers_once_and_gets_one_voucher` |
| 128 | survey_test.sql #36 | answering issues one voucher | `tests/farm_surveys.rs::a_verified_household_answers_once_and_gets_one_voucher` |
| 129 | survey_test.sql #37 | the response carries its provenance | `tests/farm_surveys.rs::a_verified_household_answers_once_and_gets_one_voucher` |
| 130 | survey_test.sql #38 | a retried submit is a replay | `tests/farm_surveys.rs::a_verified_household_answers_once_and_gets_one_voucher` |
| 131 | survey_test.sql #39 | a retried submit creates nothing new | `tests/farm_surveys.rs::a_verified_household_answers_once_and_gets_one_voucher` |
| 132 | survey_test.sql #40 | the list shows the answer and its voucher | `tests/farm_surveys.rs::a_verified_household_answers_once_and_gets_one_voucher` |
| 133 | survey_test.sql #41 | voucher codes are not readable directly | `tests/farm_vouchers.rs::only_the_household_reads_its_code` |
| 134 | survey_test.sql #42 | the audit flag is not readable | `tests/farm_vouchers.rs::only_the_household_reads_its_code` |
| 135 | survey_test.sql #43 | the household reads its own code | `tests/farm_vouchers.rs::only_the_household_reads_its_code` |
| 136 | survey_test.sql #44 | no direct response writes | `tests/farm_surveys.rs::no_route_writes_responses_directly_and_anonymous_reads_nothing` |
| 137 | survey_test.sql #45 | a second member of the same household cannot answer again | `tests/farm_surveys.rs::the_household_answers_once_whoever_answers` |
| 138 | survey_test.sql #46 | the co-member sees the household already answered | `tests/farm_surveys.rs::the_household_answers_once_whoever_answers` |
| 139 | survey_test.sql #47 | another household sees no voucher | `tests/farm_vouchers.rs::only_the_household_reads_its_code` |
| 140 | survey_test.sql #48 | another household cannot read the code | `tests/farm_vouchers.rs::only_the_household_reads_its_code` |
| 141 | survey_test.sql #49 | staff cannot read the code | `tests/farm_vouchers.rs::only_the_household_reads_its_code` |
| 142 | survey_test.sql #50 | no answers on a temporary password | `tests/farm_surveys.rs::eligibility_rules` |
| 143 | survey_test.sql #51 | submit is refused on a temporary password | `tests/farm_surveys.rs::eligibility_rules` |
| 144 | survey_test.sql #52 | a household its registrar verified is not eligible | `tests/farm_surveys.rs::eligibility_rules` |
| 145 | survey_test.sql #53 | an unverified household is not eligible | `tests/farm_surveys.rs::eligibility_rules` |
| 146 | survey_test.sql #54 | a household newer than the survey is not eligible | `tests/farm_surveys.rs::eligibility_rules` |
| 147 | survey_test.sql #55 | a person in two households is not eligible | `tests/farm_surveys.rs::eligibility_rules` |
| 148 | survey_test.sql #56 | the budget cap stops the next household | `tests/farm_surveys.rs::the_household_cap_stops_the_next_household` |
| 149 | survey_test.sql #57 | a required question must be answered | `tests/farm_surveys.rs::answers_are_checked_against_the_questions` |
| 150 | survey_test.sql #58 | an answer must be one of the options | `tests/farm_surveys.rs::answers_are_checked_against_the_questions` |
| 151 | survey_test.sql #59 | answers must match the survey questions | `tests/farm_surveys.rs::answers_are_checked_against_the_questions` |
| 152 | survey_test.sql #60 | a closed survey takes no answers | `tests/farm_surveys.rs::a_live_survey_is_frozen_and_a_closed_one_stays_closed` |
| 153 | survey_test.sql #61 | the registering officer is told they may not redeem | `tests/farm_vouchers.rs::lookups_are_staff_only_logged_and_blind_outside_the_village` |
| 154 | survey_test.sql #62 | the registering officer cannot redeem | `tests/farm_vouchers.rs::three_different_staff_and_an_id_check` |
| 155 | survey_test.sql #63 | the verifying staff member cannot redeem | `tests/farm_vouchers.rs::three_different_staff_and_an_id_check` |
| 156 | survey_test.sql #64 | an out-of-village scan finds nothing | `tests/farm_vouchers.rs::lookups_are_staff_only_logged_and_blind_outside_the_village` |
| 157 | survey_test.sql #65 | a farmer cannot look up vouchers | `tests/farm_vouchers.rs::lookups_are_staff_only_logged_and_blind_outside_the_village` |
| 158 | survey_test.sql #66 | the ID document type is required | `tests/farm_vouchers.rs::three_different_staff_and_an_id_check` |
| 159 | survey_test.sql #67 | the name check is required | `tests/farm_vouchers.rs::three_different_staff_and_an_id_check` |
| 160 | survey_test.sql #68 | a third staff member redeems, typed in lower case | `tests/farm_vouchers.rs::three_different_staff_and_an_id_check` |
| 161 | survey_test.sql #69 | the same officer retrying is a replay, not a second payout | `tests/farm_vouchers.rs::redeem_is_safe_to_repeat_and_to_race` |
| 162 | survey_test.sql #70 | a redeemed voucher cannot be redeemed again | `tests/farm_vouchers.rs::redeem_is_safe_to_repeat_and_to_race` |
| 163 | survey_test.sql #71 | the redemption is recorded with the officer name, role and ID type | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 164 | survey_test.sql #72 | the out-of-village scan is on the record | `tests/farm_vouchers.rs::lookups_are_staff_only_logged_and_blind_outside_the_village` |
| 165 | survey_test.sql #73 | the in-village scan is on the record | `tests/farm_vouchers.rs::lookups_are_staff_only_logged_and_blind_outside_the_village` |
| 166 | survey_test.sql #74 | the audit trail cannot be edited | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 167 | survey_test.sql #75 | the audit trail cannot be deleted | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 168 | survey_test.sql #76 | the farmer sees no scans or staff steps | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 169 | survey_test.sql #77 | the farmer sees who redeemed her voucher | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 170 | survey_test.sql #78 | clients cannot read events directly | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 171 | survey_test.sql #79 | field staff do not see refused scans | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 172 | survey_test.sql #80 | field staff see scans | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 173 | survey_test.sql #81 | ops sees the whole trail: registered, verified, published, answered, issued, scanned, refused, redeemed | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 174 | survey_test.sql #82 | another village sees no trail | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 175 | survey_test.sql #83 | a held voucher tells the officer it needs ops | `tests/farm_vouchers.rs::a_held_voucher_needs_ops_in_person` |
| 176 | survey_test.sql #84 | a field officer cannot redeem a held voucher | `tests/farm_vouchers.rs::a_held_voucher_needs_ops_in_person` |
| 177 | survey_test.sql #85 | an expired voucher cannot be redeemed | `tests/farm_vouchers.rs::an_expired_voucher_is_refused` |
| 178 | survey_test.sql #86 | a field officer cannot void | `tests/farm_vouchers.rs::ops_voids_with_a_reason_and_the_void_is_on_the_trail` |
| 179 | survey_test.sql #87 | admin redeems a held voucher in person | `tests/farm_vouchers.rs::a_held_voucher_needs_ops_in_person` |
| 180 | survey_test.sql #88 | a void needs a reason | `tests/farm_vouchers.rs::ops_voids_with_a_reason_and_the_void_is_on_the_trail` |
| 181 | survey_test.sql #89 | a voided voucher cannot be redeemed | `tests/farm_vouchers.rs::ops_voids_with_a_reason_and_the_void_is_on_the_trail` |
| 182 | survey_test.sql #90 | the void and its reason are on the trail | `tests/farm_vouchers.rs::ops_voids_with_a_reason_and_the_void_is_on_the_trail` |
| 183 | survey_test.sql #91 | the redemption log totals per officer per day | `tests/farm_vouchers.rs::redemptions_reconcile_per_officer_per_day_in_minor_units` |
| 184 | survey_test.sql #92 | field officers do not see the cash reconciliation log | `tests/farm_vouchers.rs::redemptions_reconcile_per_officer_per_day_in_minor_units` |
| 185 | survey_test.sql #93 | anonymous survey read denied | `tests/farm_surveys.rs::no_route_writes_responses_directly_and_anonymous_reads_nothing` |
| 186 | survey_test.sql #94 | anonymous voucher lookup denied | `tests/farm_vouchers.rs::lookups_are_staff_only_logged_and_blind_outside_the_village` |
| 187 | survey_test.sql #95 | anonymous timeline denied | `tests/farm_vouchers.rs::the_trail_is_append_only_and_filtered_by_who_asks` |
| 188 | survey_test.sql #96 | anonymous eligibility denied | `tests/farm_surveys.rs::no_route_writes_responses_directly_and_anonymous_reads_nothing` |
| 189 | survey_test.sql #97 | seeded: Neema has two surveys to answer (maize storage, harvest labour) | `tests/farm_demo.rs::the_demo_tells_the_ruaha_story` |
| 190 | survey_test.sql #98 | seeded: three vouchers issued | `tests/farm_demo.rs::the_demo_tells_the_ruaha_story` |
| 191 | survey_test.sql #99 | seeded: TZS 10,000 issued | `tests/farm_demo.rs::the_demo_tells_the_ruaha_story` |
| 192 | survey_test.sql #100 | seeded: TZS 2,000 redeemed, by Juma | `tests/farm_demo.rs::the_demo_tells_the_ruaha_story` |
| 193 | survey_test.sql #101 | seeded: TZS 8,000 outstanding | `tests/farm_demo.rs::the_demo_tells_the_ruaha_story` |
| 194 | grant_staff_test.sql #1 | grant_staff creates a login whose password is the one it printed | N/A because staff accounts come from the existing invite and sign-up flow with their own email and password; no operator script mints logins with a printed password |
| 195 | grant_staff_test.sql #2 | grant_staff creates the email identity GoTrue signs in with | N/A because staff accounts come from the existing invite and sign-up flow with their own email and password; no operator script mints logins with a printed password |
| 196 | grant_staff_test.sql #3 | a new staff login must change its password at first sign-in | N/A because staff accounts come from the existing invite and sign-up flow with their own email and password; no operator script mints logins with a printed password |
| 197 | grant_staff_test.sql #4 | a field officer is scoped to the named village | `tests/farm_tenancy.rs::a_field_officer_is_scoped_to_the_named_village` |
| 198 | grant_staff_test.sql #5 | a repeat grant for an existing login issues no new password | N/A because a farm grant issues no password: staff chose theirs when they joined the workspace |
| 199 | grant_staff_test.sql #6 | a repeat grant adds no second membership | `tests/farm_tenancy.rs::a_repeat_grant_adds_no_second_membership` |
| 200 | grant_staff_test.sql #7 | ops is granted across the whole project | `tests/farm_tenancy.rs::ops_is_granted_across_the_whole_workspace` |
| 201 | grant_staff_test.sql #8 | granting a revoked role again restores it | `tests/farm_tenancy.rs::granting_a_revoked_role_again_restores_it` |
| 202 | grant_staff_test.sql #9 | grant_staff refuses the farmer role | `tests/farm_tenancy.rs::grant_input_rules` |
| 203 | grant_staff_test.sql #10 | a field officer needs a village | `tests/farm_tenancy.rs::grant_input_rules` |
| 204 | grant_staff_test.sql #11 | admin and ops take no village | `tests/farm_tenancy.rs::grant_input_rules` |
| 205 | grant_staff_test.sql #12 | an unknown village code is refused | `tests/farm_tenancy.rs::grant_input_rules` |
| 206 | grant_staff_test.sql #13 | a malformed email is refused | N/A because the grant takes an existing member by id, not an email and name; the invite and sign-up flows refuse malformed emails and blank names |
| 207 | grant_staff_test.sql #14 | a blank name is refused | N/A because the grant takes an existing member by id, not an email and name; the invite and sign-up flows refuse malformed emails and blank names |
| 208 | village_create_test.sql #1 | ops creates a village in its own project | `tests/farm_tenancy.rs::ops_creates_a_village_and_a_retry_replays` |
| 209 | village_create_test.sql #2 | the new village carries its first planned capacity row | N/A because energy, village capacity and Control Tower energy views are out of scope (G14) |
| 210 | village_create_test.sql #3 | a created village has energy figures from the start | N/A because energy, village capacity and Control Tower energy views are out of scope (G14) |
| 211 | village_create_test.sql #4 | a retried create replays instead of inserting twice | `tests/farm_tenancy.rs::ops_creates_a_village_and_a_retry_replays` |
| 212 | village_create_test.sql #5 | a village without a planned capacity is refused | N/A because energy, village capacity and Control Tower energy views are out of scope (G14) |
| 213 | village_create_test.sql #6 | a duplicate village code in the project is refused | `tests/farm_tenancy.rs::a_duplicate_village_code_is_refused` |
| 214 | village_create_test.sql #7 | capacity can never be recorded as measured | N/A because energy, village capacity and Control Tower energy views are out of scope (G14) |
| 215 | village_create_test.sql #8 | ops cannot create a village in another project | `tests/farm_tenancy.rs::ops_cannot_create_a_village_in_another_workspace` |
| 216 | village_create_test.sql #9 | ops cannot insert a village directly into another project | `tests/farm_tenancy.rs::ops_cannot_create_a_village_in_another_workspace` |
| 217 | village_create_test.sql #10 | an officer cannot create a village | `tests/farm_tenancy.rs::officers_and_farmers_cannot_create_villages` |
| 218 | village_create_test.sql #11 | a farmer cannot create a village | `tests/farm_tenancy.rs::officers_and_farmers_cannot_create_villages` |
| 219 | village_create_test.sql #12 | a farmer cannot add a capacity row | N/A because energy, village capacity and Control Tower energy views are out of scope (G14) |
| 220 | register_cycles_test.sql #1 | two crops register as two cycles on the same plot | `tests/farm_registration.rs::two_crops_register_as_two_cycles_each_with_its_measure` |
| 221 | register_cycles_test.sql #2 | each crop carries its own expected harvest | `tests/farm_registration.rs::two_crops_register_as_two_cycles_each_with_its_measure` |
| 222 | register_cycles_test.sql #3 | the result names every cycle it created | `tests/farm_registration.rs::two_crops_register_as_two_cycles_each_with_its_measure` |
| 223 | register_cycles_test.sql #4 | crop_cycle_id stays the first cycle, for older clients | N/A because the legacy single-`cycle` payload and first-id fields served drafts typed before 5 Oct 2026 on the old app; genesy has no such clients |
| 224 | register_cycles_test.sql #5 | each cycle keeps its own measure | `tests/farm_registration.rs::two_crops_register_as_two_cycles_each_with_its_measure` |
| 225 | register_cycles_test.sql #6 | the same crop twice on one plot is refused | `tests/farm_registration.rs::crop_rules_refuse_the_whole_registration` |
| 226 | register_cycles_test.sql #7 | a second crop without its measure refuses the registration | `tests/farm_registration.rs::crop_rules_refuse_the_whole_registration` |
| 227 | register_cycles_test.sql #8 | a refused registration leaves no person behind | `tests/farm_registration.rs::crop_rules_refuse_the_whole_registration` |
| 228 | register_cycles_test.sql #9 | a single cycle + harvest payload still registers | N/A because the legacy single-`cycle` payload and first-id fields served drafts typed before 5 Oct 2026 on the old app; genesy has no such clients |
| 229 | plot_documents_test.sql #1 | an officer files a title document against a plot in their village | `tests/farm_documents.rs::an_officer_files_a_title_document_and_reads_it_back` |
| 230 | plot_documents_test.sql #2 | and can read the file back | `tests/farm_documents.rs::an_officer_files_a_title_document_and_reads_it_back` |
| 231 | plot_documents_test.sql #3 | an upload cannot be attributed to someone else | `tests/farm_documents.rs::an_upload_cannot_name_its_uploader_or_any_other_field` |
| 232 | plot_documents_test.sql #4 | a document row cannot point outside its plot folder | `tests/farm_documents.rs::a_document_row_cannot_point_outside_its_plot_folder` |
| 233 | plot_documents_test.sql #5 | a filed document cannot be renamed | `tests/farm_documents.rs::a_filed_document_is_append_only` |
| 234 | plot_documents_test.sql #6 | a filed document cannot be deleted | `tests/farm_documents.rs::a_filed_document_is_append_only` |
| 235 | plot_documents_test.sql #7 | no policy lets a filed file be replaced or deleted | `tests/farm_documents.rs::a_filed_document_is_append_only` |
| 236 | plot_documents_test.sql #8 | an officer outside the village sees no documents | `tests/farm_documents.rs::only_staff_of_the_village_see_or_file_documents` |
| 237 | plot_documents_test.sql #9 | an officer outside the village sees no files | `tests/farm_documents.rs::only_staff_of_the_village_see_or_file_documents` |
| 238 | plot_documents_test.sql #10 | an officer outside the village cannot upload into it | `tests/farm_documents.rs::only_staff_of_the_village_see_or_file_documents` |
| 239 | plot_documents_test.sql #11 | the farmer does not see title documents | `tests/farm_documents.rs::only_staff_of_the_village_see_or_file_documents` |
| 240 | plot_documents_test.sql #12 | the farmer does not see the files | `tests/farm_documents.rs::only_staff_of_the_village_see_or_file_documents` |
| 241 | plot_documents_test.sql #13 | a farmer cannot upload a document | `tests/farm_documents.rs::only_staff_of_the_village_see_or_file_documents` |
| 242 | plot_documents_test.sql #14 | ops sees the document | `tests/farm_documents.rs::only_staff_of_the_village_see_or_file_documents` |
| 243 | resource_catalogue_test.sql #1 | ops adds equipment offered for rent only | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 244 | resource_catalogue_test.sql #2 | ops adds a loan listing | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 245 | resource_catalogue_test.sql #3 | equipment must be offered to rent or to buy | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 246 | resource_catalogue_test.sql #4 | a loan range cannot run backwards | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 247 | resource_catalogue_test.sql #5 | a loan listing is retired, never deleted | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 248 | resource_catalogue_test.sql #6 | a loan listing carries no finance terms (research item C) | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 249 | resource_catalogue_test.sql #7 | an officer cannot add a loan listing | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 250 | resource_catalogue_test.sql #8 | a farmer cannot add equipment | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 251 | resource_catalogue_test.sql #9 | a farmer can read the loan listings of their project | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 252 | resource_catalogue_test.sql #10 | a machine offered only for sale cannot be requested on rent | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 253 | resource_catalogue_test.sql #11 | a machine offered only on rent cannot be requested to buy | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 254 | resource_catalogue_test.sql #12 | a farmer requests a machine on rent | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 255 | resource_catalogue_test.sql #13 | a submitted request cannot switch between rent and buy | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 256 | resource_catalogue_test.sql #14 | and stays on rent | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 257 | resource_catalogue_test.sql #15 | a rent request adds its rent per day, not a price, to the pipeline | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 258 | resource_catalogue_test.sql #16 | every seeded request is a purchase | N/A because equipment, rent/buy offers, loan listings and PUE requests are out of scope (G14) |
| 259 | supply_attached_by_test.sql #1 | before attaching, the new ops account is no name the officer may see | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 260 | supply_attached_by_test.sql #2 | a supply line records who attached it, not who the client claims | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 261 | supply_attached_by_test.sql #3 | an update cannot rewrite who attached a line, or when | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 262 | supply_attached_by_test.sql #4 | staff who can read the supply line resolve who attached it | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 263 | supply_attached_by_test.sql #5 | an officer outside the village does not resolve the attacher | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 264 | supply_attached_by_test.sql #6 | a farmer does not resolve the attacher through supply lines | N/A because market module out of scope (G14): buyers, demand, opportunities and supply lines are not ported in this phase |
| 265 | bug_report_test.sql #1 | a farmer files a bug report, filed as open under their own name | N/A because bug reports are out of scope (G14) |
| 266 | bug_report_test.sql #2 | a farmer cannot read bug reports, their own included | N/A because bug reports are out of scope (G14) |
| 267 | bug_report_test.sql #3 | a farmer cannot read bug report images | N/A because bug reports are out of scope (G14) |
| 268 | bug_report_test.sql #4 | a report cannot be filed in someone else's name | N/A because bug reports are out of scope (G14) |
| 269 | bug_report_test.sql #5 | a report needs a screenshot or a photo | N/A because bug reports are out of scope (G14) |
| 270 | bug_report_test.sql #6 | a report needs a description | N/A because bug reports are out of scope (G14) |
| 271 | bug_report_test.sql #7 | a report carries at most 30 actions | N/A because bug reports are out of scope (G14) |
| 272 | bug_report_test.sql #8 | a client cannot change a report's status | N/A because bug reports are out of scope (G14) |
| 273 | bug_report_test.sql #9 | a client cannot delete a report | N/A because bug reports are out of scope (G14) |
| 274 | bug_report_test.sql #10 | an image cannot be uploaded into someone else's folder | N/A because bug reports are out of scope (G14) |
| 275 | bug_report_test.sql #11 | no policy lets a bug report image be replaced or deleted | N/A because bug reports are out of scope (G14) |
| 276 | bug_report_test.sql #12 | an officer cannot read bug reports | N/A because bug reports are out of scope (G14) |
| 277 | bug_report_test.sql #13 | ops cannot read bug reports | N/A because bug reports are out of scope (G14) |
| 278 | bug_report_test.sql #14 | admin reads every bug report | N/A because bug reports are out of scope (G14) |
| 279 | bug_report_test.sql #15 | admin reads bug report images | N/A because bug reports are out of scope (G14) |
| 280 | bug_report_test.sql #16 | anon cannot read bug reports | N/A because bug reports are out of scope (G14) |
