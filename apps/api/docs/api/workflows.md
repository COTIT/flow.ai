# Workflows (runs, incidents, human resolutions)

A run (`workflow_executions`) is created when an approval request is approved (`approvals.md`). For WF-01 it creates one NetSuite purchase order. Contract: `docs/integrations/contracts.md` §3.2–§3.5, §5.5, §10.

Routes are under `/v1/workspace/{workspace}/workflows`. A malformed, unknown or foreign id gets 404. Coded errors have the body `{"message","code"}`. Every write takes an `Idempotency-Key` and records an append-only `workflow_resolutions` row and a `resolution` event in the same transaction.

## Execution

```jsonc
{ "id", "approval_request_id", "workflow_key", "correlation_id": "vz-<32 hex>",
  "status": "queued|running|blocked|awaiting_reconciliation|cancelled|succeeded|succeeded_with_variance|resolved_manual|failed",
  "phase": "prechecking|write_intent|write_sent|verifying|done",
  "reason": "code|null", "resolution": "linked_with_differences|closed_no_po|variance_accepted|null",
  "error_class": "transient|ambiguous|auth|permission|not_found|validation|conflict|unknown|null",
  "error_message": "readable|null", "target_ref": "netsuite:<connection>:purchaseOrder:<id>|null",
  "verified": false,            // true only for succeeded and succeeded_with_variance
  "remote_attempts": 1, "manual_retry_grants": 0,
  "allowed_actions": ["link_existing", "allow_retry", "close_no_po"],
  "created_at", "updated_at" }
```

`allowed_actions` lists the human actions open now:
- `awaiting_reconciliation`: `link_existing`, `allow_retry` (only when the latest attempt's outcome is `ambiguous`, `not_sent` or `rejected`), `close_no_po`;
- `succeeded_with_variance`: `accept_variance`, `reject_variance`;
- `blocked`: `allow_retry` (same outcome rule), `retry`.

The detail route also returns:
- `attempts`: `[{attempt_no, granted_by, grant_reason, write_intent_at, sent_at, outcome}]`;
- `events`: `[{kind, actor_kind, created_at, summary}]`, where `summary` is a readable line and the list holds the newest 200;
- `resolutions`: `[{action, actor:{user_id,name}, reason, outcome, created_at}]`.

## Routes

| method + path | permission | body | success | errors |
|---|---|---|---|---|
| GET `/executions?workflow_key=&status=&page=&page_size=` | view | — | list | 403, 422 unknown status |
| GET `/executions/{execution}` | view | — | `{"data":Execution}` + attempts, events, resolutions | 403, 404 |
| POST `/executions/{execution}/retry` | manage | `{"reason"}` | `{"data":Execution}` | 409 `execution_state_changed`, 409 `retry_needs_grant`, 422 |
| POST `/executions/{execution}/resolve` | reconcile, not the requester | `{"action","reason","target_id"?}` | `{"data":Execution}` | 403 `four_eyes_required`, 409 `execution_state_changed`, 422 (`target_id` missing or unreadable), 503 (NetSuite unreachable) |
| GET `/incidents?status=open\|resolved&kind=&page=&page_size=` | view | — | list | 403, 422 unknown status |
| POST `/incidents/{incident}/resolve` | reconcile, not the requester | `{"note"}` | `{"data":Incident}` | 403 `four_eyes_required`, 409 `incident_already_resolved`, 422 |

`reason` and `note` are required, 1–2000 characters.

### retry (`retry_after_fix`)

`blocked` → `queued` with phase `prechecking`. The reason and error are cleared and every pre-check runs again. Any other status → 409 `execution_state_changed`.

A run that already sent an attempt with a known outcome (for example a definite `rejected`) and has no unused grant → 409 `retry_needs_grant` (`An order was already attempted. A reconciler must allow one more attempt first.`). The attempt cap (`remote_attempts <= 1 + manual_retry_grants`) holds; `allow_retry` is the only way to raise it.

### resolve

"Not the requester" means the requester of the approval request behind the run. That person gets 403 `four_eyes_required` (`You cannot resolve a request you raised.`).

| action | from | to |
|---|---|---|
| `allow_retry` | `awaiting_reconciliation` or `blocked`, latest attempt outcome `ambiguous`/`not_sent`/`rejected` | `queued`, phase `prechecking`, `manual_retry_grants + 1`. The next attempt row records who granted it and why. |
| `close_no_po` | `awaiting_reconciliation` | `resolved_manual` (`closed_no_po`) |
| `accept_variance` | `succeeded_with_variance` | `resolved_manual` (`variance_accepted`); the open `tax_variance` incidents of the run are resolved with the same reason |
| `reject_variance` | `succeeded_with_variance` | unchanged; the incident stays open |
| `link_existing` | `awaiting_reconciliation` | needs `target_id` (else 422). Reads the order and verifies it (see `integrations.md` "WF-01 execute and reconcile"): match → `succeeded` (or `succeeded_with_variance`); differences → `resolved_manual` / `linked_with_differences`, never verified. An id NetSuite can't read → 422 on `target_id`; NetSuite unreachable → 503. |

Any other combination → 409 `execution_state_changed`. An attempt still in flight (no outcome) never allows a retry.

### incident resolve

`open` → `resolved` once, with `note`, `resolved_by` and `resolved_at`. There is no reopen; a recurrence opens a new incident. An already resolved incident → 409 `incident_already_resolved`.

## Engine helpers (C4)

These live in `workflows::executions`, `events`, `incidents` and `dispatch`. Ownership rules:
- `claim` (`queued` → `running`) and `takeover` set `owner_token`, which is the claiming job's `jobs.locked_by`.
- `takeover` succeeds only when no running job in the workspace holds a live lease with the old token.
- Every state write on an owned row (`queued`/`running`) is fenced on `owner_token`, so a stale worker's write changes nothing.
- `begin_attempt` locks the row in phase `prechecking`, checks the cap and that every earlier attempt has an outcome, then inserts the attempt row (the write intent) and moves the phase to `write_intent`. It returns `None` when the worker must not POST.
- `record_attempt_result` and `events::record` are not fenced, so a stale worker can still leave evidence.
- `finish_succeeded` writes the `verify_match` event in the same transaction as the status.
- The database refuses to cancel a run with `remote_attempts > 0` (CHECK I2).

## WF-01 intake (C4a)

Intake turns Odoo RFQs into approval requests. It is checked against the documented Odoo contract with an in-process fake, never against a live Odoo (gate L1). Code: `src/workflows/wf01_purchase/`.

### Jobs

| kind | started by | keys | does |
|---|---|---|---|
| `wf01_tick` | `SCHEDULES`, every minute, system job | `schedule:wf01_tick` | Enqueues the due jobs below for each `active` WF-01 pair. Same fan-out as `webhook_sweep` (`modules/webhooks/deliver.rs:204-217`). |
| `wf01_poll` | tick, every `poll_minutes` since `last_polled_at` | dedupe `wf01_poll:<pair>`, concurrency `wf01:<pair>` | Control check, then the cursor walk. |
| `wf01_sweep` | tick, every `poll_minutes` since `last_swept_at` | dedupe `wf01_sweep:<pair>`, concurrency `wf01:<pair>` | Re-reads live requests. |
| `wf01_full_scan` | tick, every `full_scan_minutes` since `last_full_scan_at` | dedupe `wf01_full_scan:<pair>`, concurrency `wf01:<pair>` | Re-reads every eligible RFQ. |
| `wf01_execute`, `wf01_reconcile` | C4b | contracts §6 | Not built: the job fails permanently (422). C4b adds the queued-execution backstop as `tick::enqueue_queued_executions`. |

The shared concurrency key means poll, sweep and full scan of one pair never run at the same time. Each job finishes as done without work when the pair is not `active` or its source connection is not `active`/`degraded`. Credential expiry is applied to both connections at job start.

### Poll

1. **Controls (R3-§2, facts O5).** The job reads `po_double_validation` and `po_double_validation_amount` of every mapped company. If any company is not `two_step` with amount 0, or the source does not return it, the pair becomes `paused` (`odoo_control_drift`) and one incident `odoo_control_drift` opens per company (subject `odoo:<connection>:res.company:<id>`). No RFQ is read.
2. **Cursor (facts O1).** It starts at the stored `(write_date, id)`, or at `activated_at − backfill_days` on the first run. Each run re-reads from `cursor − 120 s`, because a commit can surface after a later `write_date`. Pages hold 100 RFQs (`draft`/`sent`, mapped companies), ordered `write_date, id`. Equal stamps page by id.
3. **One transaction per page.** Intake of every RFQ on the page and the cursor advance commit together. A failure mid-page rolls the whole page back and the cursor stays. The overlap never moves the cursor backwards. After 20 pages the job continues in its next chunk.

### Intake rules (poll, sweep and full scan share them)

Subject: `odoo:<source connection>:purchase.order:<id>`.

| latest revision | source now | result |
|---|---|---|
| none | eligible | new revision 1 |
| `blocked`/`pending`/`approved`, no execution | same fingerprint | `blocked`: re-evaluated with current mappings (snapshot refreshed), may become `pending`; otherwise no change |
| same, no execution | other fingerprint | `invalidated` (`source_changed`), new revision |
| same, no execution | cancelled / gone / other state | `invalidated` (`source_cancelled` / `source_gone` / `source_ineligible`) |
| approved with an execution | any | the request and the execution are not changed (R3-§5); see incidents below |
| `rejected`/`returned` | same fingerprint | no new row; one `skipped_previously_decided` event per request. A resubmit makes the next revision |
| `rejected`/`returned`/`invalidated` | other fingerprint (or invalidated: any) | new revision |

Incidents once an execution exists (idempotent while open):
- Execution `awaiting_reconciliation` or phase `write_intent`/`write_sent`/`verifying`, and the source changed, left eligibility or is gone → `source_changed_during_write`.
- Execution `succeeded`, `succeeded_with_variance` or `resolved_manual` (not `closed_no_po`), updated within `succeeded_watch_days`:
  - source `to approve` or `purchase` → `duplicate_obligation_risk`;
  - source gone → `source_gone_after_execution`;
  - source still `draft`/`sent` with another fingerprint → `source_changed_after_execution`.
- Execution `queued` or `prechecking`: nothing; the executor's pre-check decides.

After the executor cancels a run and invalidates its request, the next poll (if the RFQ changes again) or full scan creates the new revision.

### Evaluate

- **Mappings.** company (subsidiary), vendor, currency, each line's product and unit, and the buyer (`user`). Every missing one is listed at once as `unmapped:<entity>:<source_id>`. A missing or non-uuid user mapping leaves no requester, and C2 adds `no_requester`.
- **Target connection** not `active`/`degraded` → `connection_inactive:<id>`.
- **Location.** `default_locations[<subsidiary target id>]` from the pair config, else null. The adapter never reads `picking_type_id`.
- **Policy, requester, approvers.** Added by C2 `upsert_revision`: `no_policy:<currency>`, `no_requester`, `no_eligible_approver`.
- **Snapshot** v1 per contracts §5.4. `name` is null (the adapter does not read it).

### Fingerprint

SHA-256 hex over canonical JSON with sorted keys and decimals as normalized strings (`3` = `3.0`). Fields: company, vendor, currency and buyer ids; non-display lines sorted by id with product, quantity, unit, price, sorted tax ids and planned date; untaxed, tax and total. `write_date`, labels, state and `picking_type_id` are left out.

### Money (facts O6, strict rule)

- Every amount is converted to integer minor units at the currency's `decimal_places`. Any value with finer digits, or below 0, blocks with `amount_not_exact`.
- **P1 `rate_precision`.** For each line, `price_unit × product_qty` must be exact in minor units.
- **P1 `untotalled_lines`.** Each line's `price_subtotal` must equal that product.
- **P2 `untotalled_lines`.** `amount_untaxed` must equal Odoo's untaxed total. When every line base is exact, `round_per_line` (Σ round(line)) and `round_globally` (round(Σ line)) agree, so the company's rounding method is not needed.
- **What blocks for now.** Discounts, price-included taxes, currencies with a rounding step that is not a power of ten (for example 0.05), and sub-cent unit prices × quantity block as `rate_precision` or `untotalled_lines`. They stay blocked until the adapter reads the discount, the rounding method and the tax facts, verified on a sandbox.
- **For C4b.** `money::odoo_untaxed(lines, PerLine | Globally, decimals)` gives Odoo's verified total. For example, Odoo's own docstring case, 2 × (12.12 × 12.12), gives 293.78 per line and 293.79 globally.

### Pair activation

`POST …/pairs/{pair}/activate` also reads the controls of every mapped company. It refuses with 422 `source_controls_unmet` when:
- a company's controls are not met, or the source does not return the company (the message names the company);
- the controls cannot be read;
- no company is mapped.
