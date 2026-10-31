# MVP proof — internal fixes and external deferrals

Status as of **25 September 2026**, against the cloud dev (demo) project.
Internal completion means the fixes below plus captured evidence. **Full MVP
sign-off remains pending** the external language and scope decisions at the
end of this page.

## Evidence captured

| check | command | result |
|---|---|---|
| migration history | `pnpm db:list` | 12 local = 12 remote, through `20260925120001`; on-disk statements identical to recorded ones |
| generated types | `pnpm db:types` | regenerated; byte-identical to committed `src/lib/db.types.ts` |
| RLS / DB contract | `pnpm db:rls` | **92 / 92 pass**, before and after E2E cleanup (60 at start of 25 Sep, 18 + 14 added) |
| typecheck | `pnpm typecheck` | pass |
| lint | `pnpm lint` | pass |
| unit | `pnpm test` | **2233 / 2233** in 105 files, both with `.env` and with it removed (CI parity) |
| build | `pnpm build` | pass |
| browser | `pnpm e2e` | final full run: **211 passed, 0 failed, 0 flaky**, 4 skipped by design (people nav smoke runs at one width). `language`, `register`, `nav` specs then run 3× with `--retries=0`: **36 / 36 each time**. Journey: **pass** (steps 1–9) |
| seeded figures | asserted by `rls_test.sql` §8 and `journey.spec.ts` | 12,000 / 6,400 / 5,600 kg · 62.2% · 10.800 kW · 489.200 kW, before and after cleanup |

CI (`.github/workflows/ci.yml`) runs typecheck, lint, unit and build
unconditionally, then the browser suite and RLS suite; missing DB or Supabase
secrets **fail** those jobs instead of skipping them. One workflow runs at a
time (`concurrency`, `cancel-in-progress: false`), and both suites upload their
logs/JSON as artifacts named by commit SHA, on success and on failure.

Coverage scope: `pnpm test:coverage` measures the unit suite only. The
officer-edit coverage reported earlier is per-feature, not whole-app.

## Requirement → implementation → test

### 1 · Identity and protected writes (P0)

| requirement | implementation | test |
|---|---|---|
| `app_user` self-update limited to `locale`, `display_name` | column grant, `20260925090001` | rls: *identity reassignment denied*, *safe self preferences remain editable* |
| full profile read self-only; scoped names for provenance | `app_user_read` = own row; `app_actor_names(ids)`; `src/lib/actorNames.ts` | rls: *full account profile is self-only*, *farmer resolves the officer…*, *cannot resolve unrelated…*, *anonymous name lookup denied*; unit: `actorNames.test.tsx`, `RequestDetailScreen.test.tsx` provenance |
| farmer read-only on person/production | `person_update_self` dropped; writes revoked | rls: *farmer verification spoof denied*, *farmer provenance spoof denied* |
| no direct writes around the RPCs, incl. relationships and receipts | revokes on 6 observed tables + `household_member`, `farm_manager`, `registration_receipt` | rls: *officer cannot bypass verification RPC*, *direct relationship injection denied* |
| RPCs run as `NOLOGIN NOBYPASSRLS` non-owner role; fixed search path; no anon | `ruaha_observed_writer`; function owner + `search_path` | rls: *RPC writer cannot login or bypass RLS*, *owns no tables*, *client cannot assume writer*, *anonymous registration/verification denied* |
| both relationship endpoints in caller scope, same village | `rpc_household_member`, `rpc_farm_manager` policies | rls: *officer cannot join another village's household*, *cross-village household/farm join denied for project staff*, *a denied registration leaves nothing behind* |
| opportunity/supply writes ops/admin only; officers keep reads | `app_manage_village`, `app_manage_opportunity`, rewritten policies + guards | rls: *officer supply write denied*, *officer opportunity update affects no rows*, *farmer opportunity write denied* |

### 2 · Provenance and market integrity (P0/P1)

| requirement | implementation | test |
|---|---|---|
| observed inserts via `withProvenance`; DB authoritative | `useSubmitRequest` uses `withProvenance`; `stamp_captured_input` trigger | rls: *farmer request provenance is normalised, not trusted* |
| `captured_by` on requests **and** buyer demand; no invented demand `source` | same trigger on `buyer_demand` | rls: *demand capture cannot be forged* |
| registration atomic, server provenance, idempotent | existing RPC via writer role | rls: *RPC registration remains usable*, *registration replay creates no duplicate*, *out-of-village registration denied* |
| corrections reset verification; only `app_verify` verifies; harvest history kept | `app_update_observed_record`, `app_supersede_harvest` | rls §9: *correction resets verification…*, *harvest correction preserves the old row*; e2e: `person-detail.spec.ts` edit test |
| opportunity machine in Postgres | `opportunity_guard` | rls: *cannot accept before sharing*, *terminal opportunity cannot reopen*, *lapsed is terminal*, *an opportunity cannot be born shared*; e2e: `opportunity.spec.ts` |
| offered total server-only | column grants + `opportunity_resum` | rls: *offered total is server-only*, *ops attaches supply and the database sums it* |
| serialised supply commitment; matching village/crop/cycle; current harvest | `opportunity_supply_guard` row locks | rls: *already-committed harvest cannot be promised…*, *supply from another village is refused*; e2e: `demand.spec.ts` over-commitment |
| concurrent attachments cannot over-commit | `for update` on opportunity then harvest | **by construction** — a single psql session cannot race two transactions; see business-rules §8 |

### 3 · One connected farmer (P0)

| requirement | implementation | test |
|---|---|---|
| journey uses the farmer it registers | the login card shown at registration (`app_farmer_login_issue`), then a forced password change (29 Sep 2026; the synthetic login fixture is gone) | `journey.spec.ts` steps 3–9: same `person_id` owns the request (checked in SQL), approval seen by that farmer, their harvest attached, Tower row drills to their record |
| Neema/Joseph isolation separate | unchanged specs | `rls_test.sql` §1, farmer specs |
| cleanup scoped to test-owned ids; accounts removed after relationships | `e2e/support/cleanup.sql` — demo-only guard, FK scan before deleting a synthetic account | global setup/teardown fail loudly on error |

### 4 · Interrupted forms (P1)

| requirement | implementation | test |
|---|---|---|
| drafts for equipment request, buyer, demand, supply, officer correction | `usePersistentForm` | `usePersistentForm.test.tsx` (11) |
| keyed by user, scope, form, record | `form:<user>:<scope>:<form>` | *another account never sees…*, *a different record scope…* |
| restore before submit; kept through failures; cleared only on success | `ready` gate, `finish()` in `onSuccess` | *is not ready until…*, *restores values and clientRef after a reload*, *a failed submit leaves the draft* |
| storage failure explicit | `FormDraftStatus` | *a storage failure is reported*, *an unreadable store is reported* |
| serialised saves; no resurrection | per-key `queueDraft` | *a late save cannot resurrect a cleared draft* |
| typing before restore finishes | early edits win and persist | *typing before the restore finishes is kept* |
| uncertain response reconciles by id | `clientRef` as row id + `recoverInsert` | `recoverInsert.test.ts` (4) |
| registration idempotency preserved | `client_ref` receipts | rls: *registration replay creates no duplicate* |

### 5 · Traceability and residue (P1/P3)

| requirement | implementation | test |
|---|---|---|
| server `committed_kg` on `v_demand_match`, once per demand/village over overlapping windows | view rewrite, `security_invoker` kept, column appended; client sum removed | rls: *demand view owns committed total*, *multi-month demand counts … once*, *one row per village*; e2e journey step 6 |
| PUE drill-downs keep village/status | `TowerScreen` links | `TowerScreen.test.tsx` |
| `/ops/tower/quality` with validated filters, same predicates as the view | `TowerQualityScreen`, `validateQualitySearch` | `TowerQualityScreen.test.tsx`, `towerSearch.test.ts` |
| headline → record within three clicks | tile → drill table → record | journey step 8 (production) |
| no `"test"` label, no `E2E-opportunity` default | removed; ownership in fixtures | cleanup keys opportunities by their marked demand |

## Operational notes from this pass

- **Seed drift repaired.** Six seeded rows had been verified outside the
  suite on 13 and 20 Sep (person `…0006`, plot `a…04`, cycle `b…05`, harvests
  `c…01`, `c…05`, `c…06`), and Neema's locale was `en`. All restored to
  `seed.sql` values by SQL; no current spec verifies outside its own records.
- `e2e/support/cleanup.sql` referenced a non-existent `registration_receipt.id`;
  fixed to `client_ref`. It had never run successfully in its new form.
- CI's `concurrency.queue` key does not exist in GitHub Actions and was
  removed; an empty `SUPABASE_DB_PORT` secret now falls back to 5432. The
  Playwright JSON report moved to `playwright-report/`, because Playwright
  empties `test-results/` at the start of every run.
- Officer correction bugs found by the browser suite and fixed: the dialog
  cleared its draft before the save resolved (`mutate` instead of
  `mutateAsync`), every record of a table shared one draft key (no
  `recordId`), plot `area_ha` was hidden by the crop-measure filter, input
  typed before the draft restore finished was dropped, and the cycle season
  was editable but never shown. Edit-dialog titles, labels and enum options
  are now translation keys (`officerEdit.fields.*`, `officerEdit.titles.*`).
- The journey's synthetic farmer needs its first-run tour marked seen;
  `TOURS_ALREADY_SEEN` lists only seeded accounts.
- Evidence logs for this pass are local run output; the durable record is the
  CI artifacts (`playwright-proof-<sha>`, `rls-proof-<sha>`) once secrets are
  set. Base commit `62adbb2`; all changes are uncommitted.

## Second pass — confirmed bugs fixed (25 Sep)

CI's `check` job failed on `f7d2ecd`; three read-only investigations, then
three implementation agents and an independent review. Only defects confirmed
by code or library source were fixed.

| bug | fix | test |
|---|---|---|
| 3 unit files crashed without `.env` (CI) | `vitest.config.ts` `test.env` placeholders on a closed port + `VITE_DATA_MODE`; `isDemoData` moved to `src/lib/dataMode.ts`; missing session mock | full suite with `.env` removed |
| language E2E reloaded before the save was sent (failed at `language.spec.ts:55`, after reload) | specs wait for the `app_user` PATCH; `trace: 'retain-on-failure'` | 3× no-retry reruns |
| nav E2E expected `/officer/register$`, but register adds `?draft=` | regex allows a query | 3× no-retry reruns |
| crop-cycle measure switch could never save | `buildEditPayload` nulls the non-matching measures; every caller passes the measure | `officerEdit.test.ts`, `useOfficerEdit.test.tsx` |
| draft not cleared if the screen unmounted mid-save (`mutate` callbacks skip without listeners, `mutationObserver.js:76`) | `mutateAsync` + `finishDraftWhenSaved` | per-form unmount tests |
| retry after an uncertain save reported success for different values | `recoverInsert` compares submitted vs stored; mismatch → `EarlierVersionSavedError`, draft cleared, lists refreshed | `recoverInsert.test.ts`, `recoverInsert.hooks.test.tsx` |
| DB silently rounded extra decimals (would also make retries mismatch); a comma price was saved as no price | decimal limits per column scale; price must be a number | schema + `DemandListScreen` tests |
| early keystrokes replaced the whole stored draft | merged over the restored draft | `usePersistentForm.test.tsx` |
| an abandoned officer draft came back over newer server data | draft carries the record's `captured_at`; a different version is discarded | `usePersistentForm`, dialog and screen tests |
| staff-scope rewrite missed `app_staff_households()`, `app_staff_opportunities()` and `fm_read` (any staff read every `farm_manager` row) | migration `20260925120001_staff_scope_followup` | 14 new RLS assertions, each shown failing before the migration |

Refuted and not changed: a stored-locale race in `LanguageSwitch`, Base UI
dropping fast clicks, forms rendering before the session loads.
Known and accepted: officer drafts written before this change carry no version
and are discarded once; the earlier-version message is translated when raised.
New English strings needing Swahili review are in `docs/i18n-handover.md`
(429 required).

## External deferrals — owner and state

| item | owner | state |
|---|---|---|
| Native review of the draft Kiswahili: every string, tour and Ops included (`docs/i18n-handover.md`, `.csv`; the flagged ones say why); 80 of 176 glossary terms unverified; DB reference labels (5 crops, 4 categories, 5 equipment) and the survey wording carry Swahili awaiting review | native reviewer | **pending**; draft ships, placeholders/plurals/incentive wording enforced by `bundles.test.ts` |
| Scope alignment with Plan v2 (exclusions and compressed roles in `CLAUDE.md`) | product owner | **pending** confirmation |
| Handset/connectivity validation, real-data consent (research A, B), finance classification (C), meter provider, season/grade/confidence taxonomies (S22) | field / legal / product | deferred |
| GitHub secrets for CI browser + RLS jobs | repository owner | required; jobs fail visibly until set |
| Fresh-database replay of all migrations | needs an approved disposable cloud project | **unproven**; the shared demo DB is never reset |

## 29 September 2026 — farmer logins, surveys and incentive vouchers

| check | command | result |
|---|---|---|
| migrations | `pnpm db:push` | `20260929090001_farmer_login`, `…02_household_four_eyes`, `…03_surveys` applied; `db.types.ts` regenerated |
| survey seed | `psql … -f supabase/seed_surveys.sql` | applied; re-runnable |
| RLS / DB contract | `pnpm db:rls` | **193 / 193 pass** (92 before, plus 101 in `survey_test.sql`) |
| unit | `pnpm test` | 2942 / 2943; the one failure, in `LanguageSwitch.test.tsx`, also fails on the previous commit (`6c88432`) and is not part of this feature |
| typecheck, lint | `pnpm typecheck`, `pnpm lint` | pass |
| browser | `pnpm e2e` | **210 passed, 1 failed, 4 skipped**. The one failure was `responsive.spec`'s phone header check, which expected the action row that the user menu (`6c88432`) folds away below `sm`; the spec now checks the menu trigger and passes (19 / 19) |
| journey | `pnpm e2e journey` | **pass, all 13 steps**: the seeded Tower figures, then survey published, answered once, refused to the registrar, redeemed by a second officer with ID seen, and every name on the ops trail |

Before these runs, four demo records that manual use on 28 Sep had changed
were restored to their seeded state: Baraka's request back to `under_review`,
Amina, Rehema, Daudi and the Sanga household back to unverified or pending,
and Salima's locale back to `sw`.
