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
| POST `/executions/{execution}/resolve` | reconcile, not the requester | `{"action","reason","target_id"?}` | `{"data":Execution}` | 403 `four_eyes_required`, 409 `execution_state_changed`, 422, 503 `link_unavailable` |
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
| `link_existing` | `awaiting_reconciliation` | needs `target_id` (else 422). Answers 503 `link_unavailable` until the read-back step is built. |

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
