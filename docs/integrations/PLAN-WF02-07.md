# Verzo glue layer: WF-02 to WF-07 plan (rev 1)

Status: plan only, no code. Written 2026-11-21 on branch `claude/verzo-wf02-07-plan`.

**Inputs**
- Stories: `~/Downloads/verzo_odoo_netsuite_workflow_stories.md` (unvalidated backlog, 2026-11-17).
- Facts: `facts/odoo-19-ops.md` (R2-O1..O11), `facts/netsuite-rest-ops.md` (R2-N1..N11), `facts/odoo-19.md` (O1..O6), `facts/netsuite-rest.md` (N1..N6, JWT, PATCH).
- WF-01: `PLAN.md` (rev 1..5), `contracts.md`, `README.md` (gates L1..L6), `SANDBOX.md`.

**Rules this plan obeys** (CLAUDE.md)
- Money is integer cents or Postgres `numeric`. **All money math (sums, extensions, variances, tolerances) is SQL.** Rust only parses provider numbers exactly (`serde_json` arbitrary_precision → `rust_decimal`) and binds them; frontends only display.
- Every table has `workspace_id` with composite FKs `(workspace_id, id)`; every route has a cross-workspace refusal test.
- Business rules live in `apps/api`. Shared UI goes in `packages/ui`. Pre-release: no feature flags.
- Strict TDD. UI changes ship Playwright tests at desktop and 375px (no console errors, no failed requests).
- Facts only. If neither a pack nor the code states it, it is **UNVERIFIED** and becomes a live gate (L7+), a facts gap (F1+, a source read), or a decision (D-xx).
- WF-01's code is reused, not changed, except where a CHECK list is widened. WF-01's `money.rs` is not touched.

---

## 1. What already exists (reused by every workflow)

| Piece | What it gives | Path |
|---|---|---|
| Connections + sealed secrets | Odoo / NetSuite connection rows, probe result, status | `apps/api/src/integrations/connections/*`, `migrations/20261119200000_integrations.up.sql` |
| Pairs | `(workflow_key, source, target)` unique; `config jsonb`; `draft/active/paused` | `src/integrations/pairs/*`; `workflow_key CHECK IN ('wf01_purchase')` |
| Mappings | per pair; entities `company, vendor, product, uom, currency, user, location` | `src/integrations/mappings/*` |
| Cursors, links | `(connection, stream)` cursor; `external_links` with `vz-<32hex>` correlation | `src/integrations/cursors.rs`, `src/integrations/links.rs` |
| Capability contract | `ErrorClass`, `ProviderError`, static capability keys, `CapabilityCheck`, neutral DTOs | `src/integrations/capability.rs` |
| Provider dispatch + fakes | live vs `INTEGRATIONS_PROVIDERS=fake` (debug only) | `src/integrations/providers/mod.rs`, `providers/fake.rs` |
| Odoo client | generic JSON-2 `call(model, method, args)`; error map, 429 and 5xx → `Transient` | `providers/odoo/client.rs:120`, `providers/odoo/errors.rs:88` |
| NetSuite client | M2M ES256 JWT, `get_json`, `q=externalId IS` lookup; 429 → `Transient` | `providers/netsuite/{auth,mod,sink}.rs`, `providers/netsuite/errors.rs:76` |
| Approvals (generic) | policies, requests (`subject_ref`, fingerprint, revision), append-only decisions, four-eyes | `src/modules/approvals/*`; `workflow_key CHECK IN ('wf01_purchase')` |
| Execution contract | `claim`, `takeover`, `transition`, `begin_attempt` (write intent before HTTP), `grant_for`, `finish_succeeded` | `src/workflows/executions.rs:61-271` |
| Events, incidents, resolutions | append-only `workflow_events`; `workflow_incidents` open once per kind+subject; `workflow_resolutions` | `src/workflows/{events,incidents}.rs`, migration `20261119210000_approvals_workflows.up.sql:165-235` |
| WF-01 jobs | `wf01_tick/poll/sweep/full_scan/execute/reconcile` | `src/jobs/kinds.rs:67-77,110-164,237-242` |
| Doc-copied fakes | 127.0.0.1 fake Odoo and NetSuite with request logs | `src/workflows/wf01_purchase/fake_{odoo,netsuite}.rs` |
| Live smoke kit | `#[ignore]` tests per gate | `src/integrations/live.rs:148-582` |
| Permissions | `integration.{approve, approve_high, manage, reconcile, view}` | `src/modules/permissions/catalog.rs:113-114` |
| UI | `ApprovalList`, `PurchaseApprovalCard`, `ExecutionStatus`, `IncidentBadge`; `/approvals/purchases` in Office and Flow; Office `/settings/integrations/[pair]/mappings` | `packages/ui/src/components/business-approvals/*`, `apps/{office,flow}/app/(app)/approvals/purchases/`, `apps/office/app/(app)/settings/integrations/` |
| Verzo tool layer | Tools = OpenAPI operations listed in a reviewed policy file (`disposition`, `risk`, `authorization`), generated into the catalog. WF-01 routes are all `internal` (not assistant tools) | `apps/office/lib/voice/operation-policy.json`, `apps/office/scripts/generate-voice-catalog.mjs`, `apps/office/lib/voice/catalog.{ts,generated.json}` |
| Scheduled reports | member's own read-only reports by email; `report CHECK IN ('sales_summary','attention','overdue_invoices','stock')` | `src/modules/scheduled_reports/*`, `migrations/20261109120000_scheduled_reports.up.sql:6` |

**Absent for WF-02..07:** any read of receipts, bills, stock, sales or invoices; a generic read snapshot; review cases; expectation tracking; report runs; workflow definitions; any assistant-exposed integration tool.

---

## 2. Cross-cutting pieces (build once, in the first child that needs them)

| ID | Piece | Shape | First user |
|---|---|---|---|
| X1 | **Read snapshot** `ext_records` | one row per fetched record or line: `connection_id, kind CHECK(...), ext_id, parent_ext_id, item_ref, uom_ref, location_ref, currency_ref, qty numeric, amount numeric, doc_date date, event_at timestamptz, state text, source_modified_at, fetched_at, read_run_id, raw jsonb (≤64KB)`. Unique `(ws, connection_id, kind, ext_id, read_run_id)`. Numbers stored as `numeric` from exact decimals; never float | WF-05 |
| X2 | **Read runs** `ext_read_runs` | `purpose, connection_id, status CHECK(running/succeeded/partial/failed), pages, rows, error_class, started_at, finished_at`. `partial` when a documented ceiling (R2-N8: 100,000 rows) or page limit is hit; the UI says so; never silent truncation | WF-05 |
| X3 | **Review cases** `review_cases` + append-only `review_case_decisions` | `workflow_key, subject_ref, reason, status CHECK(open/awaiting_correction/accepted/resolved_verified/superseded), evidence jsonb (cites `ext_records` ids)`. Partial unique `(ws, workflow_key, subject_ref, reason) WHERE status IN ('open','awaiting_correction')` = duplicate events cannot open twice | WF-02 |
| X4 | **Incidents + recovery** (WF-03) | widen `workflow_incidents.kind`, add `category, diagnosis, verified_at, resolution_kind`; new `exchange_expectations`. Every workflow that writes registers an expectation | WF-03 |
| X5 | **Capability keys** | new static keys per read/write (below) + probe checks; Odoo module presence via `fields_get` (R2-O9: bot cannot read `ir.module.module`); NetSuite via `metadata-catalog?select=` (R2-N10) | each WF |
| X6 | **Widened CHECK lists** | one reversible migration per child widens `workflow_key` on pairs, approval_policies, approval_requests, workflow_executions; and `integration_mappings.entity` (adds `stock_location`, `customer`) | each WF |
| X7 | **Fetch helpers** | Odoo paging: `search_read` with `limit/offset/order` (O1). NetSuite collection paging: `limit/offset`, offset divisible by limit, `hasMore` (R2-N8). 429 → `Transient` → job retry with existing backoff (N5; Odoo rate limits UNVERIFIED, R2 #12, same handling) | WF-05 |

**Shared rules**
- **Writes:** only WF-06 (Odoo `action_replenish`) and WF-07 action steps write. Each write is an execution with the existing at-most-once attempt row (rule W1, PLAN P-1/P-2) and registers an X4 expectation.
- **Never:** release, create or approve a payment; adjust stock by default; PATCH/PUT a record another system owns.
- **Labels:** every screen states the data age ("as of <fetched_at>"), the time zone, the currency, and "verified against documented contract" until the gates pass.
- **Permissions:** no new keys. Reads and reports use `integration.view`. Review cases and incidents use `integration.reconcile`. Proposals use `integration.approve`. Definitions use `integration.manage`.
- **Contracts:** before B1 the parent appends the frozen names (tables, enums, routes, error codes, job kinds) to `contracts.md` as §11, as WF-01 Wave 0 did.

---

## 3. WF-02 PO, receipt and vendor bill matching

### 3.1 Exists vs gap
- **Exists:** connections, pairs, mappings (`product`, `uom`, `currency`, `vendor`), Odoo client, NetSuite client, WF-01 `external_links` (Odoo RFQ ↔ NetSuite PO).
- **Gap:** reads of PO lines with received/billed quantities, receipts and bills; the matcher; tolerances; review cases (X3); UI.

### 3.2 Process and ownership

| Question | Rule (default; see D-21..D-25) |
|---|---|
| System of record for the triad | **The PO owner's system holds PO, receipts and bills.** The pair config `triad_owner = odoo \| netsuite` picks it. Odoo triad is fully fact-backed; NetSuite triad waits on L7 + L10 |
| What Flow reads | PO header + lines, receipts (done), bills (all non-cancelled), product control policy |
| What Flow writes | **Nothing.** "Ready" is a Flow label only (D-22) |
| Payment | **Never released.** Flow shows native payment state read-only (Odoo `payment_state`, R2-O3) |
| Quantities | Odoo: `product_qty`, `qty_received`, `qty_invoiced` are all in the line's `product_uom_id` (R2-O2) → **no UoM conversion**. NetSuite: line `units` is a string (R2-N1/N2); Flow compares only when units are identical, else case `uom_differs` |
| Currency | compare in document currency only; a bill whose currency ≠ PO currency → case `currency_mismatch`. No FX |
| Taxes | untaxed only (O6 `price_subtotal`) |
| Original values | every result cites the `ext_records` rows it used; raw payload kept |

### 3.3 Facts used

| Call / field | Fact | Verdict | Gate |
|---|---|---|---|
| `purchase.order` `state`, `invoice_status`, `invoice_ids`, `picking_ids`, `receipt_status` | R2-O2 | VERIFIED (code) | L11 |
| `purchase.order.line` `qty_received` (done moves, returns subtracted), `qty_invoiced` (non-cancel bills, **drafts count**), `qty_to_invoice`, `invoice_lines`, `move_ids` | R2-O2 | VERIFIED (code) | L11 |
| `product.template.purchase_method` (`purchase` / `receive`) | R2-O2 | VERIFIED (code) | L11 |
| `stock.picking` `state`, `date_done`, `backorder_id`, `return_id`; `stock.move` `purchase_line_id`, `quantity`, `product_uom` | R2-O1 | VERIFIED (code) | L11 |
| `account.move` `move_type in (in_invoice,in_refund)`, `state`, `payment_state`; line `purchase_line_id` | R2-O3 | VERIFIED (code) | L11 |
| Bill line `quantity`, `price_unit`, `price_subtotal`, `product_uom_id` | not in any pack | UNVERIFIED | **F1** |
| NetSuite PO line `quantityReceived`, `quantityBilled`, `matchBillToReceipt`, `billVarianceStatus` | R2-N3 | PARTIAL (read-only + pending-bill counting unverified) | **L7** |
| NetSuite `itemReceipt.createdFrom`, line `orderLine`, `quantity`, `units` | R2-N1 | VERIFIED (fields) | L10 (listing by PO) |
| NetSuite bill line `orderDoc`, `orderLine`, `billReceipts` meaning | R2-N2 | UNVERIFIED | **L7** |
| Bill status meanings (`approvalStatus 11,1,2,3`, `status A..F`) | R2-N2 | UNVERIFIED | shown raw, labelled "NetSuite code" |
| Native controls: Odoo `account_3way_match` absent from Community; NetSuite 3 Way Match "partially received… not supported" | R2-O4, R2-N4a | PARTIAL / VERIFIED quote | D-23 |
| Read rights (bot groups) | R2-O10, R2-N11 | PARTIAL | L11, L13 |

### 3.4 Data model, states, jobs, routes
- **Migration** `<ts>_wf02_match.{up,down}.sql` (reversible; widens `workflow_key` with `wf02_match`):
  - `match_tolerances(pair_id, currency_ref, qty_pct numeric ≥0, qty_abs numeric ≥0, amount_pct numeric ≥0, amount_abs_minor bigint ≥0)`, unique `(ws, pair_id, currency_ref)`.
  - `match_runs(pair_id, po_ref, read_run_id, inputs_hash char(64), status CHECK(running/done/failed))`, unique `(ws, pair_id, po_ref, inputs_hash)` → a duplicate event with unchanged inputs creates no run.
  - `match_lines(run_id, po_line_ref, ordered_qty, received_qty, billed_qty numeric, ordered_amount, billed_amount numeric NULL, control CHECK(ordered/received), result CHECK(matched/within_tolerance/awaiting_receipt/awaiting_bill/mismatch/unverifiable), reasons text[])`. Filled by **one `INSERT … SELECT`** over X1 rows and tolerances: control qty = `ordered_qty` if `purchase_method='purchase'` else `received_qty` (mirrors `qty_to_invoice`, R2-O2); variance and tolerance tests are SQL.
- **Result → case** (X3): `mismatch` / `unverifiable` opens a case per `(po_line_ref, reason)`. Reasons: `over_billed`, `over_received`, `price_variance` (after F1), `uom_differs`, `currency_mismatch`, `unmapped_product`, `stale_snapshot`.
- **Case states:** `open` → `accepted` (variance accepted, reason required) | `awaiting_correction` (correction requested) → `resolved_verified` (a later run shows `matched`/`within_tolerance`) | `superseded` (PO cancelled or line gone). A decision on a case whose PO `write_date` (Odoo) / `lastModifiedDate` (NetSuite) is newer than the snapshot → **409 `snapshot_stale`**.
- **Jobs:** `wf02_match_scan` (`Every::Minutes(60)`, per active pair, `concurrency_key = pair`): finds POs changed since cursor (O1 `write_date` polling), enqueues `wf02_match_po` (`dedupe_key = pair:po_ref`).
- **Routes** (`integration.view` reads, `integration.reconcile` decisions):
  - `GET /workspace/{ws}/workflows/matches?pair=&result=` · `GET …/matches/{po_ref}` · `POST …/matches/{po_ref}/rerun`
  - `GET /workspace/{ws}/review-cases?workflow_key=` · `POST /workspace/{ws}/review-cases/{id}/decide {decision: accept|request_correction, reason, snapshot_seen}`
  - `GET|PUT /workspace/{ws}/integrations/pairs/{pair}/tolerances`
- **UI:** Office `/integrations/matching` (list + PO detail with three columns ordered / received / billed per line, source references). Shared `MatchTable`, `ReviewCaseCard` in `packages/ui/src/components/integrations/`. Not a Verzo tool in v1.

### 3.5 Acceptance tests
| Test | Proves |
|---|---|
| `wf02_full_match_is_matched` | ordered = received = billed → `matched`, no case |
| `wf02_partial_receipt_receive_policy_awaits_receipt` | 10 ordered, 4 received, 4 billed, `receive` policy → `awaiting_receipt`, no case |
| `wf02_billed_over_received_opens_case` | 4 received, 6 billed → `mismatch(over_billed)`, one case |
| `wf02_purchase_policy_bills_against_ordered` | `purchase` policy, 10 ordered, 0 received, 10 billed → `matched` |
| `wf02_draft_bill_counts_and_is_labelled` | draft bill included (R2-O2 rule), label "draft" |
| `wf02_refund_reduces_billed` | `in_refund` subtracts (R2-O2) |
| `wf02_return_reduces_received` | purchase return lowers `qty_received` |
| `wf02_line_uom_needs_no_conversion` | PO line in dozens, receipt move in units → compare uses `qty_received` in line UoM |
| `wf02_netsuite_units_differ_opens_case` | `units` strings differ → `uom_differs`, no numeric compare |
| `wf02_currency_mismatch_opens_case` | bill EUR, PO USD → `currency_mismatch` |
| `wf02_tolerance_math_in_sql` | 2% qty tolerance, 101 vs 100 → `within_tolerance`; assertion runs the SQL, Rust holds no arithmetic |
| `wf02_duplicate_event_no_second_run_or_case` | same inputs twice → 1 run, 1 case |
| `wf02_stale_snapshot_decision_409` | PO changed after snapshot → decide returns `snapshot_stale` |
| `wf02_request_correction_then_verified` | case → `awaiting_correction` → next run matched → `resolved_verified` |
| `wf02_no_payment_or_write_calls` | fake Odoo/NetSuite logs contain reads only |
| `wf02_permission_denied_on_decide` | `integration.view` only → 403 |
| `wf02_odoo_403_marks_unverifiable` | fake returns AccessError → `unverifiable`, readable message |
| `wf02_429_retries_then_succeeds` | 429 twice then 200 → run done, attempts recorded |
| `wf02_pagination_over_bill_lines` | 250 lines over 3 pages → all matched |
| `wf02_other_workspace_refused` | every route → 404 for another workspace |
| Playwright `wf02-matching.spec.ts` | list, detail, accept variance, stale message; desktop + 375px |

### 3.6 Decisions
- **D-21 Triad owner.** Default: the PO owner's system holds the whole triad (pair config `triad_owner`). *Alt:* cross-system triad (Odoo receipts + NetSuite bills); needs a receipt↔PO link no pack states.
- **D-22 Flow write on discrepancy.** Default: none; read-only. *Alt:* Odoo `action_toggle_block_payment` on a bill with an open case (R2-O4, callable over JSON-2); needs an Odoo write key and a new gate L15.
- **D-23 Native controls.** Default: Flow matching runs alongside native controls and only displays native fields. *Alt:* skip POs where the customer runs NetSuite 3 Way Match (it rejects partial receipts, R2-N4a).
- **D-24 Default tolerance.** Default: 0 (exact) for qty and amount until the customer sets values. *Alt:* a customer-provided % / absolute per currency at setup.
- **D-25 Draft bills.** Default: count them, as Odoo `qty_invoiced` does (R2-O2), labelled "draft". *Alt:* posted only; needs F1 and per-line state reads.

---

## 4. WF-03 Failed synchronization detection and controlled recovery

### 4.1 Exists vs gap
- **Exists:** execution states, attempts, W1 at-most-once, takeover, reconcile job, manual `retry`/`resolve` routes, incidents (8 kinds, open once), resolutions, connection probe status (`active/degraded/unauthorized/expired/disconnected`), `ErrorClass` maps for both providers, correlation ids `vz-<32hex>`.
- **Gap:** a record of *expected* exchanges with a deadline; time-based "late" detection; readable categories and diagnosis; incident list screen; "close only after verified reconciliation" as a database rule; health-driven incidents.

### 4.2 Process and ownership

| Question | Rule |
|---|---|
| Which exchanges are watched | Flow-run writes only (WF-01 executions, WF-06 actions, WF-07 action steps) (D-31) |
| Identity | correlation id + source ref + target external id (existing `external_links`) |
| Detection | (a) failures already classed at the call; (b) `wf03_expect_sweep` finds expectations past `due_at`; (c) `wf03_health` probes every connection |
| Change detection source | polling only: Odoo `write_date` (O1); NetSuite `lastModifiedDate` / SuiteQL after L8/L10. Odoo webhooks are unsigned, 1 s timeout, no retry (R2-O9) → not trusted (D-33) |
| Retry | transient before write intent: job retry (existing backoff, `max_attempts`). After write intent: never automatic (W1); only the existing human grant (`integration.reconcile`, non-requester, reason) |
| Permanent failures | `Validation`, `Permission`, `Auth`, `Conflict` → no auto retry; incident |
| Authoritative changes | Flow never PATCHes or PUTs (WF-01 non-goal kept). A retry re-sends the *same create* with the *same* external id only after reconciliation found no record |
| Close | `resolved` with `resolution_kind = 'reconciled'` requires `verified_at` (read-back matched); `manual_closed` requires a reason and actor |

### 4.3 Facts used
| Item | Fact | Verdict | Gate |
|---|---|---|---|
| Odoo change detection by `write_date` + `id` | O1 | VERIFIED (code) | L1 |
| Odoo webhook delivery semantics | R2-O9 | VERIFIED (code) | — (not used) |
| Odoo `mail.message` chatter readable by non-admin; old/new values admin-only | R2-O9 | PARTIAL | not used |
| NetSuite `lastModifiedDate` on every record; filterable only with `x-ns-filterable` | R2-N10, N2 | PARTIAL | **L10** |
| NetSuite system notes need a named permission | R2-N10 | VERIFIED quote | not used in v1 |
| NetSuite push events | R2-N10 | UNVERIFIED (absence only) | not used |
| 429 `CONCURRENCY_LIMIT_EXCEEDED`, error body | N5, N6 | VERIFIED / PARTIAL | L5 |
| NetSuite async job status endpoints | R2-N10 | VERIFIED | not used (no async writes) |

### 4.4 Data model, states, jobs, routes
- **Migration** `<ts>_wf03_recovery.{up,down}.sql`:
  - `workflow_incidents`: widen `kind` with `exchange_late, exchange_failed, auth_failure, permission_denied, rate_limited, mapping_invalid`; add `category CHECK(transient/authorization/data/conflict/missing/unknown_outcome)`, `diagnosis jsonb`, `verified_at`, `resolution_kind CHECK(reconciled/manual_closed)`; `CHECK (resolution_kind <> 'reconciled' OR verified_at IS NOT NULL)`.
  - `exchange_expectations(workflow_key, correlation_id, execution_id NULL, source_ref, target_kind, target_external_id, due_at, status CHECK(pending/met/late/failed/closed), checks smallint, last_checked_at, incident_id NULL)`, unique `(ws, correlation_id)`.
- **Category map** (in `apps/api`, tested per class): `Transient`→transient (429 → `rate_limited`), `Auth`/`Permission`→authorization, `Validation`→data (`mapping_invalid` when the provider detail names a mapped field), `Conflict`→conflict, `NotFound`→missing, `Ambiguous`/`Unknown`→unknown_outcome.
- **Diagnosis** = plain sentence + impacted records (`subject_ref`, target id, last attempt evidence). No raw codes on screen; the code lives in `diagnosis.code`.
- **Expectation states:** `pending` → `met` (read-back matched) | `late` (past `due_at`, incident `exchange_late`) → `met` | `failed` (permanent class) → `closed` (incident resolved).
- **Jobs:** `wf03_expect_sweep` (`Every::Minutes(5)`): for each `pending`/`late` expectation past due, call the target `read`/`find_by_external_id` capability once; bounded by `checks ≤ 10` then stays `late` for humans (same rule as R3-§6c). `wf03_health` (`Every::Minutes(15)`): runs each connection probe; `Auth`/`Permission` → incident and connection status change.
- **Routes:** `GET /workspace/{ws}/workflows/incidents?category=&status=` (extend existing), `GET …/incidents/{id}` (diagnosis + timeline from `workflow_events`), `POST …/incidents/{id}/verify` (runs one read-back now), existing `…/resolve` gains `resolution_kind`. Retry stays the existing `POST …/executions/{id}/retry`.
- **UI:** Office `/integrations/incidents` (list by category, detail with timeline and the existing retry/resolve actions). Flow shows only the open-incident count in its attention strip (Flow stays simple).

### 4.5 Acceptance tests
| Test | Proves |
|---|---|
| `wf03_expectation_met_on_verified_readback` | create → read-back matches → `met`, no incident |
| `wf03_late_expectation_opens_one_incident` | due passed, target missing → `late`, 1 `exchange_late` incident; repeated sweeps add none |
| `wf03_late_then_met_auto_verifies` | target appears later → `met`, incident `resolved(reconciled)` with `verified_at` |
| `wf03_reconciled_close_without_verify_rejected` | DB CHECK refuses `reconciled` without `verified_at` |
| `wf03_permanent_failure_not_retried` | `Validation` → `failed`, 0 further attempts after 3 sweeps |
| `wf03_429_is_transient_and_backs_off` | 429 → `rate_limited` category, job retried with backoff, no incident if later OK |
| `wf03_ambiguous_write_never_auto_resent` | timeout after write intent → 1 POST total, incident `unknown_outcome` |
| `wf03_manual_retry_one_post_same_eid` | grant by non-requester → exactly 1 more POST, same external id |
| `wf03_retry_by_requester_403` | requester grant → 403 |
| `wf03_target_changed_since_send_no_write` | read-back differs → `target_conflict`, 0 PATCH/PUT in fake log |
| `wf03_health_auth_failure_incident` | probe `Auth` → connection `unauthorized`, `auth_failure` incident |
| `wf03_check_budget_stops_at_10` | 10 checks then no more calls; stays visible |
| `wf03_category_map_per_error_class` | one case per `ErrorClass` |
| `wf03_other_workspace_refused` | incidents/expectations of another workspace → 404 |
| Playwright `wf03-incidents.spec.ts` | list, detail, verify, resolve; readable text, no raw codes; desktop + 375px |

### 4.6 Decisions
- **D-31 Watched scope.** Default: Flow-run exchanges only. *Alt:* also third-party connector traffic via a customer-declared rule (source model, target record type, external-id pattern); needs L10.
- **D-32 Expected delay (`due_at`).** Default: 15 minutes after write intent. *Alt:* a customer value per workflow.
- **D-33 Odoo webhooks.** Default: not accepted (polling only). *Alt:* accept as an early-poll hint only (never as data) on an unguessable URL; needs the loop check in R2 UNVERIFIED #9 as a gate.

---

## 5. WF-04 Cross-system inventory discrepancy monitoring

### 5.1 Exists vs gap
- **Exists:** connections, pairs, `product` and `uom` mappings, clients, X1/X2 (from B1), X3 (from WF-02).
- **Gap:** stock reads on both sides, a `stock_location` mapping, a shared quantity definition, freshness, comparison, investigation cases, UI.

### 5.2 Process and ownership

| Question | Rule (default; D-41..D-44) |
|---|---|
| System of record for stock by location | customer-declared per mapped location pair (D-42); Flow does not assume both should match |
| Compared definition | **`on_hand` only** (D-41): Odoo Σ `stock.quant.quantity` over the explicitly mapped internal locations ↔ NetSuite per-location `quantityOnHand` |
| Why not "available" by default | Odoo available = quantity − reserved (R2-O5); NetSuite available = on hand − committed and "never tracked as a negative quantity" (R2-N5). Different definitions → not compared unless the customer confirms |
| Units | Odoo quant quantity is in the product's default UoM (R2-O5). Compare only when that UoM is mapped (`uom`) to the unit NetSuite reports; which unit NetSuite reports per location is **L9** |
| Freshness | each side carries `fetched_at`; a pair older than the window is `stale` and not compared |
| What Flow writes | **Nothing by default.** Stock adjustments are off (D-44) |
| Explanations | Odoo recent done movements for the item and locations (R2-O5); NetSuite movements after L8 |

### 5.3 Facts used
| Call / field | Fact | Verdict | Gate |
|---|---|---|---|
| `stock.quant` `location_id`, `quantity`, `reserved_quantity`, `available_quantity`, `in_date`; read for `base.group_user` | R2-O5, R2-O10 | VERIFIED (code) | L11 |
| `stock.quant.product_id` | not in pack | UNVERIFIED | **F3** |
| `stock.location.usage`, `warehouse_id`; `stock.warehouse.lot_stock_id` | R2-O5 | VERIFIED (code) | L11 |
| Recent movements: `stock.move.line` `state='done'`, `date` desc, `is_inventory`, location usages | R2-O5 | VERIFIED (code) | L11 |
| NetSuite `GET /inventoryItem/{id}?expandSubResources=true`; `locations` element `location`, `quantityOnHand`, `quantityAvailable`, `quantityCommitted`, `quantityOnOrder` | R2-N5 | PARTIAL (fields VERIFIED in RAB; unit of per-location quantity UNVERIFIED) | **L9** |
| NetSuite quantity definitions | R2-N5 | VERIFIED quote | — |
| NetSuite stock SuiteQL tables `inventoryBalance` etc. | R2-N5 | UNVERIFIED | not used |
| NetSuite `inventoryAdjustment` create; permission | R2-N6 | VERIFIED create / UNVERIFIED permission | L16 (only if D-44 alt) |

### 5.4 Data model, states, jobs, routes
- **Migration** `<ts>_wf04_stock.{up,down}.sql` (widens `workflow_key` with `wf04_stock`; adds mapping entity `stock_location`):
  - `stock_location_scopes(pair_id, odoo_location_ids text[], netsuite_location_id text, expected_equal bool, label)` — the explicit mapping; unique `(ws, pair_id, netsuite_location_id)`.
  - `stock_checks(pair_id, odoo_read_run_id, netsuite_read_run_id, definition CHECK('on_hand'), freshness_minutes int, status CHECK(running/done/failed))`.
  - `stock_check_lines(check_id, item_map_id, scope_id, odoo_qty numeric, netsuite_qty numeric, diff numeric, odoo_fetched_at, netsuite_fetched_at, result CHECK(equal/within_threshold/discrepancy/stale/unit_unmapped/not_expected_equal))`, filled by one `INSERT … SELECT` (sum of quants, diff and threshold in SQL).
- **Discrepancy → case** (X3, reason `stock_discrepancy`), evidence = quant rows, NetSuite location row, last 20 done move lines. Case decisions: `explained` (reason) | `correction_requested` → `resolved_verified` when a later check is `equal`/`within_threshold`.
- **Jobs:** `wf04_stock_check` (`Every::Minutes(60)` per active pair; manual run route). NetSuite reads one GET per mapped item (no list filter needed); per-connection `concurrency_key` respects the account limit (N5).
- **Routes:** `GET|PUT /workspace/{ws}/integrations/pairs/{pair}/stock-scopes`, `POST /workspace/{ws}/workflows/stock-checks` (run now), `GET …/stock-checks/{id}?result=`, review cases as in WF-02.
- **UI:** Office `/integrations/stock` (scope setup, latest check with stale/discrepancy filters, case detail citing records).

### 5.5 Acceptance tests
| Test | Proves |
|---|---|
| `wf04_equal_on_hand_no_case` | 12 vs 12 → `equal` |
| `wf04_sum_over_mapped_locations_only` | quants in an unmapped child location are excluded |
| `wf04_discrepancy_opens_case_with_evidence` | 12 vs 9 → case citing quant ids, NetSuite location, recent moves |
| `wf04_stale_side_not_compared` | NetSuite fetch older than window → `stale`, no case |
| `wf04_unmapped_unit_not_compared` | product default UoM unmapped → `unit_unmapped` |
| `wf04_available_never_compared_by_default` | only `on_hand` rows exist |
| `wf04_not_expected_equal_listed_only` | scope `expected_equal=false` → listed, no case |
| `wf04_no_adjustment_calls` | fake logs: GET only, no `inventoryAdjustment` POST, no Odoo write |
| `wf04_duplicate_run_one_open_case` | two checks with same discrepancy → one open case |
| `wf04_threshold_math_in_sql` | threshold 1, diff 1 → `within_threshold` |
| `wf04_netsuite_404_item_unverifiable` | mapped item gone → line `discrepancy` blocked with readable reason |
| `wf04_429_backoff` / `wf04_permission_denied_403` | error classes surface, run `failed` with category |
| `wf04_odoo_paging_quants` | 1,200 quants over pages → correct sum |
| `wf04_other_workspace_refused` | 404 |
| Playwright `wf04-stock.spec.ts` | setup, run, stale badge, case; desktop + 375px |

### 5.6 Decisions
- **D-41 Definition.** Default: `on_hand` only. *Alt:* also `available`, shown with a "different definitions" label.
- **D-42 Expected to match.** Default: per scope `expected_equal`, false until the customer sets it (listed, not alerted). *Alt:* compare every mapped scope.
- **D-43 Window and threshold.** Default: freshness 60 min, threshold 0. *Alt:* customer values per scope.
- **D-44 Adjustments.** Default: none; investigation case only. *Alt:* NetSuite `inventoryAdjustment` after a Flow approval, as an execution with W1; needs L16. Odoo adjustments are not in any pack and are not offered.

---

## 6. WF-05 Unified operational and financial reporting

### 6.1 Exists vs gap
- **Exists:** clients, `external_links` (WF-01 RFQ ↔ PO), scheduled-report runner, the Verzo tool catalog pipeline.
- **Gap:** X1/X2, report templates, join rules, report runs with traceable rows, totals in SQL, Verzo read tools, saved reports and alerts.

### 6.2 Process and ownership

| Question | Rule (default; D-51..D-54) |
|---|---|
| Who defines the query | **Fixed templates** in `apps/api`. Verzo picks a template and fills typed params; it never writes SQL or a provider filter |
| First templates | `fulfilled_without_invoice` (the story's example) and `rfq_vs_po` (WF-01 links) |
| Joins | only through a declared join rule (D-51) or `external_links`. 0 candidates → `unmatched`; >1 → `ambiguous` (listed, never picked) |
| Final accounting values | NetSuite (D-52); Odoo amounts shown as "operational" |
| Time zone | workspace time zone (D-53); Odoo datetimes are naive UTC (R2-O7), NetSuite record datetimes are UTC (R2-N8); NetSuite `Date` fields have no zone |
| Currency | totals per currency, computed in SQL; no cross-currency sum, no FX |
| Permissions | `integration.view` on the pair; reads use the connection's bot rights |
| Volume | documented ceilings (R2-N8: 100,000 SuiteQL rows; offset divisible by limit) → run `partial` with a message |
| Writes | none to either ERP |

### 6.3 Facts used
| Call / field | Fact | Verdict | Gate |
|---|---|---|---|
| Delivery = `stock.picking` with type code `outgoing`, `state='done'`, `date_done` | R2-O1, R2-O6 | VERIFIED (code) | L11 |
| Filter by type: read `stock.picking.type` (`code`, read for `base.group_user`), then `picking_type_id in ids` | R2-O1, R2-O10 | VERIFIED (code) | L11 |
| `stock.picking.sale_id` (stored compute) | R2-O6 | VERIFIED (code) | L11 |
| `sale.order` `state`, `invoice_status`, `delivery_status`, `effective_date` | R2-O6 | VERIFIED (code) | L11 |
| `sale.order` `name`, `partner_id`, `currency_id`, `amount_untaxed`; line compute rules | not in pack | UNVERIFIED | **F2** |
| Odoo sales visibility: `group_sale_salesman` sees own orders only | R2-O10 | VERIFIED (code) | bot uses `account.group_account_readonly` (L11) |
| Odoo aggregation methods exist (`formatted_read_group`) | R2-O7 | VERIFIED (code) | **not used**: totals are ours, in SQL |
| NetSuite `invoice` `createdFrom`, `tranDate`, `currency`, `total`, `subtotal`, `amountRemaining`, `externalId` | R2-N7 | VERIFIED (RAB) | L13 |
| NetSuite collection filter `q=tranDate ON_OR_AFTER "m/d/yyyy"` | R2-N8, N2 | PARTIAL (field filterability) | **L10** |
| NetSuite SO `externalId` lookup by `q=externalId IS` | N2 | PARTIAL | L2 |
| SuiteQL columns, type codes, session time zone | R2-N8 | UNVERIFIED | **L8** (not used until passed) |

### 6.4 Data model, states, jobs, routes, Verzo tools
- **Migration** `<ts>_wf05_reports.{up,down}.sql` (creates X1 `ext_records`, X2 `ext_read_runs`; widens `workflow_key` with `wf05_report`):
  - `report_join_rules(pair_id, rule_key CHECK('so_external_id_template'), template text)`; e.g. NetSuite `salesOrder.externalId = 'odoo-so-' || <sale.order id>` (customer-declared; D-51).
  - `report_definitions(pair_id, template_key CHECK(fulfilled_without_invoice/rfq_vs_po), params jsonb, saved bool, created_by)`.
  - `report_runs(definition_id NULL, pair_id, template_key, params jsonb, tz text, date_from date, date_to date, status CHECK(queued/running/succeeded/partial/failed), odoo_read_run_id, netsuite_read_run_id, as_of)`.
  - `report_rows(run_id, left_record_id, right_record_ids uuid[], match_state CHECK(matched/unmatched/ambiguous), currency_ref, amount numeric)`. Totals are a SQL `GROUP BY currency_ref, match_state` at read time; nothing stored as a precomputed total.
- **Jobs:** `wf05_report_run` (on demand, `dedupe_key = run id`). Saved alerts: widen `scheduled_reports.report` with `cross_erp_exceptions` (reuses its mailer and slots; D-54).
- **Routes** (`integration.view`):
  - `GET /workspace/{ws}/reports/cross/templates` · `POST /workspace/{ws}/reports/cross/runs {template_key, pair_id, date_from, date_to}` → 202 + run id · `GET …/runs/{id}` (rows, totals per currency, filters, tz, as-of, source links) · `POST|GET /workspace/{ws}/reports/cross/definitions`
- **Verzo tools** (`apps/office/lib/voice/operation-policy.json`, regenerate the catalog):
  - `reports_cross_templates_list`, `reports_cross_runs_create`, `reports_cross_runs_get`: `disposition: "execute"`, `risk: "read"`, `allOf: ["integration.view"]`, `retry.mode: "reconcile_only"`.
  - `reports_cross_definitions_create`: `risk: "write"` (Flow row only), confirmation field `template_key`.
  - Verzo's answer must quote the run's tz, date range, currency totals and the `unmatched/ambiguous` counts from the API; it never computes a figure itself.
- **UI:** Office `/reports/cross` (template picker, run table, totals per currency, ambiguous badge, links to source records). Flow: the Verzo answer card links to the same run.

### 6.5 Acceptance tests
| Test | Proves |
|---|---|
| `wf05_fulfilled_without_invoice_basic` | 3 done deliveries, 2 invoiced → 1 unmatched row with source links |
| `wf05_ambiguous_join_listed_not_picked` | 2 NetSuite SOs share a key → `ambiguous`, excluded from matched totals |
| `wf05_totals_per_currency_in_sql` | USD and EUR rows → two totals; no combined figure |
| `wf05_week_boundary_in_workspace_tz` | delivery at 23:30 UTC on Sunday, workspace UTC+2 → falls in next week |
| `wf05_netsuite_date_field_not_shifted` | `tranDate` used as a plain date |
| `wf05_partial_at_documented_ceiling` | fake caps rows → run `partial` + message |
| `wf05_pagination_offset_divisible_by_limit` | request sequence respects R2-N8 |
| `wf05_429_retries_run_completes` | transient then OK |
| `wf05_odoo_permission_denied_readable` | AccessError → run `failed`, category `authorization` |
| `wf05_no_writes_either_side` | fake logs: reads only |
| `wf05_verzo_tool_policy_read_only` | catalog test: the 3 run tools are `risk: read`, require `integration.view` |
| `wf05_verzo_cannot_pass_raw_filter` | unknown param → 422 |
| `wf05_saved_alert_scheduled_reports` | `cross_erp_exceptions` mails only the member's own login |
| `wf05_cancelled_delivery_excluded` | `state='cancel'` never appears |
| `wf05_other_workspace_refused` | 404 on runs, definitions |
| Playwright `wf05-cross-report.spec.ts` | run, read totals, ambiguous badge; desktop + 375px |

### 6.6 Decisions
- **D-51 Order ↔ invoice join.** Default: a customer-declared template for NetSuite `salesOrder.externalId` from the Odoo order id, then invoice `createdFrom` = that SO (R2-N7). *Alt:* only Flow-created links (`external_links`), so only Flow-made records join.
- **D-52 Final accounting values.** Default: NetSuite. *Alt:* Odoo.
- **D-53 Report time zone.** Default: workspace time zone. *Alt:* the asking member's own time zone.
- **D-54 Exception alert channel.** Default: `scheduled_reports` email to the member's own login. *Alt:* in-app attention item only.

---

## 7. WF-06 Low-stock replenishment and purchase proposal

### 7.1 Exists vs gap
- **Exists:** WF-01 (Odoo draft RFQ → Flow approval → NetSuite PO), executions with W1, approvals, `external_links` of WF-01 RFQs, WF-04 stock reads.
- **Gap:** orderpoint and vendor-price reads, proposal rows, overlap checks, the `action_replenish` write, UI.

### 7.2 Process and ownership

| Question | Rule (default; D-61..D-64) |
|---|---|
| Who owns stock, reorder rules, purchasing | Odoo owns stock and orderpoints; NetSuite owns the final PO via WF-01 (D-61) |
| Reuse ERP rules | Flow reads Odoo `qty_to_order` (already nets open RFQs and confirmed POs, R2-O8); Flow computes no forecast |
| Auto orderpoints | skipped: Odoo already drafts RFQs for `trigger='auto'` on its own schedule (R2-O8) (D-62) |
| Duplicate prevention | (1) `qty_to_order` nets in-progress RFQs; (2) `action_replenish` merges into a matching draft RFQ (R2-O8 `_run_buy`); (3) one proposal per orderpoint while open; (4) W1 at-most-once write; (5) block when a WF-01-linked RFQ for the product exists (D-63) |
| Missing supplier | no `product.supplierinfo` and no orderpoint `supplier_id` → proposal `blocked(missing_supplier)` before any write (Odoo would raise `ProcurementException`, R2-O8) |
| Estimated cost | `supplierinfo.price × qty` computed in SQL, rounded to the currency's decimals in SQL, labelled "estimate"; price is unrounded in Odoo (R2-O8, O6) |
| What Flow writes | after proposal approval: one `stock.warehouse.orderpoint.action_replenish` call → draft RFQ; the spend approval stays WF-01 |
| NetSuite | read-only: per-location `quantityOnOrder` shown as a coverage hint; >0 needs reviewer acknowledgement |

### 7.3 Facts used
| Call / field | Fact | Verdict | Gate |
|---|---|---|---|
| Orderpoint `trigger`, `product_min_qty`, `product_max_qty`, `qty_forecast`, `qty_to_order`, `supplier_id`, `route_id` | R2-O8 | VERIFIED (code) | L11 |
| Orderpoint `product_id`, `location_id`, `warehouse_id`, `company_id` | not in pack | UNVERIFIED | **F4** |
| `_quantity_in_progress` counts `draft/sent/to approve` RFQ lines | R2-O8 | VERIFIED (code) | L11 |
| `action_replenish` public, JSON-2 callable; merges into draft RFQ or creates one as SUPERUSER; stays `draft` | R2-O8 | VERIFIED (code) | **L12** (rights) |
| Rights to call `action_replenish` with a least-privilege bot | R2-O10 | UNVERIFIED | **L12** |
| Which cron runs `_run_scheduler_tasks` | R2-O8 | UNVERIFIED | **F4** (timing only) |
| `product.supplierinfo` `partner_id`, `min_qty`, `price`, `currency_id`, `date_start/end`, `delay` | R2-O8 | VERIFIED (code) | L11 |
| NetSuite per-location `quantityOnOrder`, `reorderPoint`, `preferredStockLevel` | R2-N5, R2-N9 | VERIFIED (RAB) | L9 |
| NetSuite Order Items / supply plan over REST | R2-N9 | UNVERIFIED | not used |

### 7.4 Data model, states, jobs, routes
- **Migration** `<ts>_wf06_replenish.{up,down}.sql` (widens `workflow_key` with `wf06_replenish`):
  - `replenish_proposals(pair_id, orderpoint_ref, product_ref, location_ref, supplier_ref NULL, qty numeric, uom_ref, unit_price numeric NULL, currency_ref NULL, est_cost numeric NULL, rationale jsonb (min, max, forecast, in-progress, `qty_to_order`, read run ids), status CHECK(draft/blocked/pending_approval/approved/rejected/executing/done/superseded), blocked_reasons text[], approval_request_id NULL, execution_id NULL, result_rfq_ref NULL)`. Partial unique `(ws, pair_id, orderpoint_ref) WHERE status IN ('draft','blocked','pending_approval','approved','executing')`.
- **States:** `draft` → `blocked` (missing_supplier / rfq_linked_to_netsuite_po / netsuite_on_order_unacknowledged / unmapped) | `pending_approval` → `approved` → `executing` → `done` (RFQ read back) | `rejected`; any → `superseded` when the next scan shows `qty_to_order = 0`.
- **Execution:** `approved` creates a `workflow_executions` row; `begin_attempt` before the call (W1); verify = re-read orderpoint `qty_to_order` and the draft RFQ lines for the product; ambiguous outcome → reconciliation, never a second call without a human grant; registers an X4 expectation.
- **Jobs:** `wf06_scan` (`Every::Minutes(60)`, `concurrency_key = pair`), `wf06_execute` (`dedupe_key = execution id`), `wf06_reconcile` (job backoff, ≤10).
- **Routes:** `GET /workspace/{ws}/workflows/replenishment?status=`, `POST …/replenishment/scan`, `POST …/replenishment/{id}/submit` (creates the approval request), `POST …/replenishment/{id}/acknowledge` (NetSuite on-order hint); approve/reject via existing `/approvals/requests/{id}/…`.
- **UI:** Office `/integrations/replenishment`; proposal card shows the calculation lines; approvals appear in the existing approvals list with a "Replenishment proposal" label.

### 7.5 Acceptance tests
| Test | Proves |
|---|---|
| `wf06_proposal_uses_odoo_qty_to_order` | quantity equals Odoo `qty_to_order`; rationale lists inputs |
| `wf06_auto_orderpoint_skipped` | `trigger='auto'` → no proposal |
| `wf06_missing_supplier_blocks_before_write` | no supplierinfo → `blocked`, 0 Odoo writes |
| `wf06_unapproved_stays_draft` | no decision → no `action_replenish` call |
| `wf06_approved_calls_replenish_once` | 1 call; draft RFQ read back → `done` |
| `wf06_duplicate_scan_one_open_proposal` | two scans → one proposal |
| `wf06_concurrent_execute_one_call` | 8 workers → 1 call (claim CAS + W1) |
| `wf06_timeout_after_call_reconciles` | ambiguous → reconcile reads RFQ, 1 call total |
| `wf06_wf01_linked_rfq_blocks` | product has a WF-01 linked draft RFQ → `blocked(rfq_linked_to_netsuite_po)` |
| `wf06_netsuite_on_order_needs_ack` | `quantityOnOrder > 0` → blocked until acknowledged |
| `wf06_estimate_math_in_sql` | 3 × 1.335 in a 2-decimal currency → SQL-rounded estimate, label "estimate" |
| `wf06_superseded_when_qty_zero` | next scan `qty_to_order=0` → `superseded` |
| `wf06_permission_denied_submit_and_approve` | missing permission → 403; requester cannot approve (four-eyes) |
| `wf06_odoo_procurement_exception_readable` | fake raises the documented message → `failed`, category `data` |
| `wf06_other_workspace_refused` | 404 |
| Playwright `wf06-replenishment.spec.ts` | scan, submit, approve, done; desktop + 375px |

### 7.6 Decisions
- **D-61 Ownership.** Default: Odoo stock + orderpoints; Flow drafts the Odoo RFQ; WF-01 approves spend and creates the NetSuite PO. *Alt:* NetSuite-native replenishment; no REST operation runs Order Items (R2-N9), so Flow would only report.
- **D-62 Auto orderpoints.** Default: skipped. *Alt:* listed as info.
- **D-63 WF-01 RFQs stay draft in Odoo.** Fact: WF-01 forbids confirming the RFQ, so after the NetSuite PO exists the Odoo draft keeps counting in `_quantity_in_progress` (R2-O8). Default: block proposals for products with such RFQs until the RFQ is cancelled in Odoo by the customer. *Alt:* adopt WF-01's D2b (Flow cancels the RFQ after success; Odoo write key).
- **D-64 Approvals.** Default: one proposal approval (`integration.approve`) before `action_replenish`, plus the WF-01 spend approval. *Alt:* no proposal approval; WF-01's approval is the only gate.

---

## 8. WF-07 Configurable order-to-fulfillment orchestration

### 8.1 Exists vs gap
- **Exists:** static capability registry (`capability.rs`), generic executions/attempts/events (W1), approvals, incidents/expectations (X4), the Verzo tool policy pipeline with `risk` and `confirmationFields`.
- **Gap:** versioned definitions, a step library bound to capabilities, a validator, a run engine with pinned versions, pause/resume, manual hand-off, Verzo draft tools, UI.

### 8.2 Process and ownership

| Question | Rule (default; D-71..D-74) |
|---|---|
| Who authors | Verzo turns the user's words into a **draft spec** by calling a tool; the API validates it. Verzo never executes steps |
| Step kinds | `trigger`, `condition`, `approval`, `action`, `wait_until`, `manual_handoff`. Conditions use a fixed operator set over earlier step outputs; no code, no expressions |
| Allowed actions | only registered capabilities with `side_effect`, `required_permission`, `required_fields`, `provider`, `owner_system`, `gate` metadata. Unknown → validation error; capability not available on the connection's last probe → `manual_handoff` or activation blocked |
| Dangerous actions | any `side_effect=true` step must be preceded by an `approval` step; the validator refuses otherwise |
| Versioning | `draft` → `published` (immutable, DB trigger) → `retired`. A run pins its version; editing creates a new version; in-flight runs never change |
| Ownership | SO and invoice in NetSuite, stock reads in Odoo (D-72) |
| First process | the story's wholesale example, restricted to existing capabilities (D-71) |

**Step library v1** (each maps to an existing or planned capability):

| Step | Capability | Kind | Facts | Gate |
|---|---|---|---|---|
| Artwork approval | Flow approval | approval | — | — |
| Check stock | WF-04 Odoo quant read | read | R2-O5 | L11 |
| Request purchasing if short | WF-06 proposal | action (via WF-06) | R2-O8 | L12 |
| Wait for verified fulfillment | Odoo delivery done (`state='done'`) or NetSuite `itemFulfillment` read | wait_until | R2-O6, R2-N7 | L11 / L13 |
| Invoice after fulfillment | NetSuite `POST /salesOrder/{id}/!transform/invoice` (204), body may set `externalId` | action | R2-N7, ops pack §0 "Transforms" | **L14** |
| Hand off | manual task with instructions | manual_handoff | — | — |

### 8.3 Facts used
| Item | Fact | Verdict | Gate |
|---|---|---|---|
| Transforms available, URL form, 204, body sets fields | ops pack §0 "Transforms", R2-N7 | VERIFIED | L14 (externalId honoured on transform, duplicate lookup) |
| SO → fulfillment needs `inventoryLocation` (cross-subsidiary link) | R2-N7 | PARTIAL | L14 (not used: fulfillment is read only) |
| "If Advanced Shipping is not enabled… an invoice is created at the same time" | R2-N7 | VERIFIED quote | L14 (detect existing invoice before transform) |
| Status letter meanings for SO/fulfillment/invoice | R2-N7 | UNVERIFIED | conditions may not branch on them |
| Odoo `sale.order` fields | F2 | UNVERIFIED | F2 |
| Capability discovery: `fields_get`; metadata catalog | R2-O9, R2-N10 | VERIFIED / PARTIAL | L13 |

### 8.4 Data model, states, jobs, routes, Verzo tools
- **Migration** `<ts>_wf07_definitions.{up,down}.sql` (widens `workflow_key` with `wf07_custom`):
  - `workflow_definitions(key, title, status CHECK(draft/published/retired), created_by)`, unique `(ws, key)`.
  - `workflow_definition_versions(definition_id, version int, spec jsonb (≤64KB), spec_hash char(64), validation jsonb, created_by, published_by NULL, published_at NULL)`; trigger forbids UPDATE once `published_at` is set; `CHECK (published_by IS NULL OR published_by <> created_by)` (D-74).
  - `workflow_runs(version_id, trigger_ref, correlation_id, status CHECK(running/waiting_approval/waiting_data/waiting_handoff/failed/succeeded/cancelled), current_step int)`.
  - `workflow_run_steps(run_id, step_no, step_key, capability_key, status CHECK(pending/running/waiting/done/failed/skipped/handed_off), input jsonb, output jsonb, approval_request_id NULL, execution_id NULL)`, unique `(run_id, step_no)`.
- **Engine:** `wf07_tick` (`Every::Minutes(1)`) advances runs one step at a time with a CAS on `current_step`; action steps create a `workflow_executions` row (W1, X4 expectation); waits poll on their own read capability with a deadline → `waiting_data` incident when late.
- **Routes** (`integration.manage` for definitions, `integration.view` for runs):
  - `POST /workspace/{ws}/workflows/definitions/validate` (spec in, report out: unknown steps, missing fields, owner system per step, dangerous steps, gates not passed)
  - `POST …/definitions` (create draft version) · `GET …/definitions/{id}` · `POST …/definitions/{id}/versions/{v}/test` (dry run against fakes/recorded snapshots, no writes) · `POST …/versions/{v}/publish` · `POST …/definitions/{id}/retire`
  - `GET /workspace/{ws}/workflows/runs?definition=` · `GET …/runs/{id}` · `POST …/runs/{id}/resume` · `POST …/runs/{id}/cancel`
- **Verzo tools** (policy file): `workflows_definitions_validate` (`risk: read`), `workflows_definitions_create` (`risk: write`, confirmation fields `title`, `steps`), `workflows_runs_list`/`_get` (`risk: read`). **`publish`, `resume`, `cancel` stay `internal`** (Office UI only; humans publish).
- **UI:** Office `/integrations/workflows` (spec view as a step list with owner system and danger marks, validation report, test run, publish, run history with step timeline). Flow shows paused runs that wait on the current user.

### 8.5 Acceptance tests
| Test | Proves |
|---|---|
| `wf07_validate_unknown_capability_blocks` | unknown step → error, cannot publish |
| `wf07_side_effect_without_approval_refused` | invoice step not preceded by approval → validation error |
| `wf07_unavailable_capability_becomes_handoff` | probe says missing → step `manual_handoff` or publish blocked (per D-73) |
| `wf07_published_version_immutable` | UPDATE on published version → DB error |
| `wf07_inflight_run_keeps_version` | publish v2 while run on v1 → run finishes on v1 |
| `wf07_publisher_not_author` | same user → 403 (D-74) |
| `wf07_run_pauses_at_approval_and_resumes` | waits, approve, continues |
| `wf07_missing_data_pauses_with_incident` | wait deadline passed → `waiting_data`, incident |
| `wf07_invoice_transform_once` | 1 transform POST even with 8 workers (W1) |
| `wf07_invoice_exists_already_no_transform` | existing invoice for SO → step `done` without POST |
| `wf07_ambiguous_transform_reconciles` | timeout → no second POST without grant |
| `wf07_test_run_makes_no_writes` | dry run: fake logs reads only |
| `wf07_verzo_create_requires_confirmation` | catalog: `workflows_definitions_create` has confirmation fields; publish is `internal` |
| `wf07_verzo_draft_cannot_publish` | tool path cannot reach publish |
| `wf07_cancel_does_not_undo_side_effects` | cancel after invoice → run `cancelled`, invoice untouched, event recorded |
| `wf07_other_workspace_refused` | 404 on definitions and runs |
| Playwright `wf07-workflows.spec.ts` | validate, test, publish, run timeline; desktop + 375px |

### 8.6 Decisions
- **D-71 First process.** Default: the wholesale example restricted to the v1 step library. *Alt:* the customer names another repeatable process before B4 starts.
- **D-72 Ownership.** Default: NetSuite SO + invoice, Odoo stock. *Alt:* Odoo-native order-to-invoice; Odoo invoice creation is not in any pack (new facts round first).
- **D-73 Steps whose live gate is open.** Default: read steps may be published with a "documented contract only" label; side-effect steps may not be published until their gate passes. *Alt:* allow publishing with an explicit warning acknowledged by the publisher.
- **D-74 Publish four-eyes.** Default: publisher ≠ author of that version. *Alt:* one person may publish.

---

## 9. New live gates and facts gaps

Live gates follow the README rule: a gate passes only when its `#[ignore]` test in `apps/api/src/integrations/live.rs` runs green against a real sandbox and the observed blocks are copied into the facts pack. SANDBOX.md gets one section per gate.

| Gate | What it proves | Test | Blocks |
|---|---|---|---|
| L7 | NetSuite triad links: bill line `orderDoc`/`orderLine`/`billReceipts` point to the PO, PO line and receipts; PO line `quantityReceived`/`quantityBilled` are read-only and count pending-approval bills | `live_l7_netsuite_bill_receipt_links` | WF-02 NetSuite triad |
| L8 | SuiteQL `transaction`/`transactionLine` column names, type codes, base vs transaction currency columns, session time zone of date columns (needs Records Catalog permission) | `live_l8_netsuite_suiteql_catalog` | NetSuite movements (WF-04), SuiteQL paths (WF-03/05) |
| L9 | `GET inventoryItem/{id}?expandSubResources=true` returns the `locations` element with `quantityOnHand`/`quantityAvailable`/`quantityCommitted`/`quantityOnOrder`, and which unit those numbers use | `live_l9_netsuite_inventory_locations` | WF-04 compare, WF-06 hint |
| L10 | `x-ns-filterable` in the metadata catalog for `tranDate`, `lastModifiedDate`, `createdFrom` on `invoice`, `itemReceipt`, `vendorBill`, `itemFulfillment`, `salesOrder`; a `q=` date filter returns the expected rows | `live_l10_netsuite_filterable_fields` | WF-05 NetSuite side, WF-02 receipt listing, D-31 alt |
| L11 | Odoo least-privilege read bot (`account.group_account_readonly` + `stock.group_stock_user`) reads every R2-O1..O8 model used here; combined record rules (R2 UNVERIFIED #6) | `live_l11_odoo_ops_reads` | production claim for WF-02/04/05/06 Odoo side |
| L12 | `action_replenish` with the least-privilege write bot: required groups, draft RFQ merge, `ProcurementException` shape | `live_l12_odoo_action_replenish` | WF-06 production |
| L13 | NetSuite role permission and level for GET/list on `itemReceipt`, `vendorBill`, `invoice`, `itemFulfillment`, `salesOrder`, `inventoryItem` (extends L5) | `live_l13_netsuite_ops_read_permissions` | WF-02/04/05/07 NetSuite reads |
| L14 | `salesOrder` → `invoice` transform: 204, `externalId` set in the body is stored, `q=externalId IS` finds it, behaviour when an invoice already exists (Advanced Shipping off) | `live_l14_netsuite_so_invoice_transform` | WF-07 invoice step |
| L15 *(only if D-22 alt)* | Odoo `action_toggle_block_payment` with the write bot; NetSuite `paymentHold` permission | `live_l15_payment_block` | D-22 alt |
| L16 *(only if D-44 alt)* | NetSuite `inventoryAdjustment` create permission and level | `live_l16_netsuite_inventory_adjustment` | D-44 alt |


**Facts gaps (source reads, docs only; an R3 facts child closes them before B1.2):**
- **F1** Odoo `account.move.line` `quantity`, `price_unit`, `price_subtotal`, `product_id`, `product_uom_id`, `display_type`; `account.move` `partner_id`, `currency_id`, `invoice_date`, `amount_untaxed`, `name`.
- **F2** Odoo `sale.order` `name`, `partner_id`, `currency_id`, `amount_untaxed`, `date_order`; `sale.order.line` `_compute_qty_delivered` / `_compute_invoice_status` (R2 UNVERIFIED #11).
- **F3** Odoo `product_id` on `stock.quant`, `stock.move`, `stock.move.line`; `picking_type_id` on `stock.picking` (stored or not).
- **F4** Odoo `stock.warehouse.orderpoint` `product_id`, `location_id`, `warehouse_id`, `company_id`; the cron that runs `_run_scheduler_tasks` and its interval (R2 UNVERIFIED #8).
- A gap the source read cannot close becomes a live gate before the dependent child merges.

---

## 10. Build waves

Rules (memory + WF-01 practice): **one heavy child at a time** (laptop RAM rule); each child = its own worktree (`use_worktree: true`), plans first and waits for parent approval, commits locally, never pushes; strict TDD; runs only its own tests + `cargo fmt`, `clippy -D warnings`, `cargo sqlx prepare --check` through `~/.genesy/heavy.sh` with the shared `CARGO_TARGET_DIR`; one reversible migration per child, timestamped after the latest on the merged tip; shared hot files (`jobs/kinds.rs`, `permissions/catalog.rs`, `operation-policy.json`, migrations) touched only by the child that owns that wave step. The parent merges, runs the full gate locally on the merged tip, then pushes.

| Step | Child | Scope | Depends on | Model |
|---|---|---|---|---|
| B0a | parent | `contracts.md` §11: frozen names for X1..X7 and every table, enum, route, job kind, error code below | this plan approved | — |
| B0b | R3 facts | F1..F4 (docs only) | — | Opus 5.5 medium |
| B1.1 | WF-03 + X4/X5/X6 | incidents widening, expectations, health job, incident UI | B0a | Opus 5.5 high (concurrency) |
| B1.2 | WF-05 + X1/X2/X7 | snapshots, read runs, fetch helpers, templates, report runs, Verzo read tools, scheduled alert | B1.1, B0b (F2, F3) | Opus 5.5 medium |
| B2.1 | WF-02 + X3 | matcher, tolerances, review cases (Odoo triad; NetSuite triad stubbed until L7) | B1.2, B0b (F1, F3) | Opus 5.5 medium |
| B2.2 | WF-04 | stock scopes, checks, cases | B2.1 (X3) | Opus 5.5 medium |
| B3 | WF-06 | proposals, `action_replenish` execution, reconcile | B2.2, B0b (F4) | Opus 5.5 high (at-most-once write) |
| B4 | WF-07 | definitions, validator, engine, Verzo draft tools | B3 | Fable 5.2 high |
| B5 | sandbox | L7..L14 live runs (REV 5: sandbox last) + facts update | sandboxes exist | parent |

**Why this order holds (fact-based):** WF-03 and WF-05 need no unverified write behaviour and create the pieces others reuse (X4 for every writer, X1/X2 for every reader). WF-02 and WF-04 need X1 and add X3. WF-06 needs WF-04's stock reads and WF-01 links (D-63). WF-07 composes WF-04, WF-06 and X4. The proposed order is kept unchanged.

---

## 11. Non-goals

- Releasing, creating, scheduling or approving any payment (`vendorPayment`, Odoo register payment).
- Stock adjustments in either ERP by default (D-44 alt only, behind approval and L16).
- PATCH/PUT on records another system owns; bidirectional field sync; FX conversion; tax mapping.
- Free-form SQL, SuiteQL or Odoo domains from Verzo; LLM-computed figures.
- Demand forecasting; driving NetSuite Order Items, Demand Planning or MRP (no REST operation, R2-N9).
- Driving native approval workflows (NetSuite 3 Way Match, Odoo `account_3way_match`).
- Inbound webhooks (D-33 alt only); NetSuite system notes; observing third-party connectors (D-31 alt only).
- Odoo 20 support (source not checked, R2-O11).
- A general expression language or code steps in WF-07; branching on undocumented status letters.
- Changes to genesy's own purchasing, stock, invoices or WF-01's money logic.
- Any "integrated with Odoo/NetSuite" claim before the gates pass.

---

## 12. Readiness verdict

**Buildable now against doc-copied fakes** (fakes copy only what the packs state):

| Workflow | Buildable now | Gated |
|---|---|---|
| WF-03 | all of it (uses WF-01 facts and existing code) | production claim: L1, L5 |
| WF-05 | Odoo side (after F2/F3), NetSuite invoice reads via record API, templates, totals, Verzo read tools | NetSuite date filter L10; permissions L13; SuiteQL L8 not used |
| WF-02 | Odoo triad quantity matching (after F1/F3 for amounts), tolerances, cases, UI | **NetSuite triad not built until L7 + L10** (link semantics are undocumented; a fake would be guesswork) |
| WF-04 | Odoo quants, NetSuite per-location fields from RAB, compare, cases, UI | unit semantics L9; NetSuite movements L8; permissions L11/L13 |
| WF-06 | proposals, overlap checks, `action_replenish` execution with W1 | rights L12; F4 for orderpoint fields |
| WF-07 | definitions, validator, engine, approval/wait/handoff steps, Verzo draft tools | invoice transform step L14; SO fields F2 |

**Open before B1:** approval of this plan; D-21..D-74 (all have defaults); the parent's B0a contracts. **Open before any production claim:** L7..L14 plus the existing L1..L6.
