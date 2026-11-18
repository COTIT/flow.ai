# Approvals (WF-01 purchase requests)

Approval requests come from the WF-01 intake: an Odoo RFQ becomes a request in Flow. An approver who is not the requester approves, rejects or returns it. An approval queues one workflow run (see `workflows.md`). Contract: `docs/integrations/contracts.md` §3.1, §5.3, §5.4.

Routes are under `/v1/workspace/{workspace}/approvals`. Ids are uuids. An id that is malformed, unknown or in another workspace gets 404 `The requested resource could not be found.` Coded errors have the body `{"message","code"}`. The UI shows a readable sentence per `code` and never the code itself.

## Permissions

| name | lets the member |
|---|---|
| `integration.view` | see every request and the policies |
| `integration.approve` | decide requests below the policy threshold |
| `integration.approve_high` | decide requests at or above the threshold; it also covers `integration.approve` |
| `integration.reconcile` | see every request (resolution routes are in `workflows.md`) |
| `integration.manage` | manage policies; resubmit any request |

The workspace owner and `workspace_admin` hold every permission, but **four-eyes still applies**: nobody can decide a request they raised, the owner included.

## Requests

### GET `/requests?workflow_key=&status=&scope=all|mine|decidable&page=&page_size=`

Any active member. Newest first, 50 per page (max 100).

- `scope=all` (the default) lists every request. It needs one of view, approve, approve_high, reconcile or manage. Without one, the scope is forced to `mine`.
- `scope=mine`: requests the caller raised.
- `scope=decidable`: `pending` requests the caller may decide (not raised by them, and they hold the needed permission).
- `status` must be one of `blocked`, `pending`, `approved`, `rejected`, `returned` or `invalidated`, else 422 `{"errors":{"status":["The selected status is invalid."]}}`.

List rows leave out `snapshot`.

### GET `/requests/{request}`

The full request with `snapshot`. Members with an integration permission see any request. Other members see only requests they raised; anything else is 404.

### ApprovalRequest

```jsonc
{ "id", "workflow_key": "wf01_purchase", "pair_id", "subject_ref": "odoo:<connection>:purchase.order:<id>",
  "revision": 1, "fingerprint": "64 hex",
  "status": "blocked|pending|approved|rejected|returned|invalidated",
  "blocked_reasons": [{ "code": "unmapped", "entity": "vendor", "source_id": "42" }],
  "invalidated_reason": "source_changed|source_cancelled|source_gone|source_ineligible|null",
  "amount": { "minor": 125000, "currency_ref": "EUR", "decimals": 2 },   // untaxed, integer minor units
  "required_permission": "integration.approve|integration.approve_high|null",  // null only while blocked
  "requester": { "user_id", "name" } | null,
  "decision": { "decision": "approve|reject|return", "actor": { "user_id", "name" }, "comment", "created_at" } | null,
  "can_decide": true,          // pending, caller is not the requester, caller holds the permission
  "execution": { "id", "status", "phase", "reason", "resolution", "error_class", "error_message",
                 "target_ref", "verified", "updated_at" } | null,
  "open_incidents": [{ "id", "kind" }],
  "snapshot": { … },           // detail only, contracts §5.4
  "created_at", "updated_at" }
```

The approval status, the run status (`execution`) and the open incidents are separate fields. An approved request whose run failed shows `status: approved` next to the run's own status. `verified` is true only for `succeeded` and `succeeded_with_variance`.

`blocked_reasons` codes: `unmapped:<entity>:<source_id>`, `no_policy:<currency>`, `no_requester`, `no_eligible_approver`, `amount_not_exact`, `rate_precision`, `untotalled_lines`, `connection_inactive:<connection_id>`. A code with a value after it puts that value in `entity` (for example `no_policy:EUR` gives `{"code":"no_policy","entity":"EUR"}`).

### POST `/requests/{request}/approve` · `/reject` · `/return`

Body `{"fingerprint": "64 lowercase hex", "comment": "…"}`. `comment` is optional on approve and required on reject and return (max 2000). Send an `Idempotency-Key`: a retry with the same key and body replays the first answer.

Checks, in order:
1. The caller raised the request → 403 `four_eyes_required` (`You cannot decide a request you raised.`). This applies to the owner too.
2. The caller lacks the request's `required_permission` → 403 `This action is unauthorized.` Holding `integration.approve_high` also passes `integration.approve`.
3. A reject or return without a comment → 422 `{"errors":{"comment":["The comment field is required."]}}`.
4. The decision is a compare-and-set on `status = 'pending' AND fingerprint = <sent>`. If no row changes: a request that is no longer pending → 409 `approval_already_decided`; a changed fingerprint → 409 `approval_stale` (`This request changed after you opened it. Reload to see the new version.`).

On success (200 `{"data": ApprovalRequest}`), one transaction writes:
- the request's new status, `decided_by` and `decided_at`;
- one `approval_decisions` row (append-only; one per request);
- a `decision` event;
- on approve, one `workflow_executions` row (`queued`, phase `prechecking`, new `correlation_id`), and the run's job is enqueued.

Two approvers deciding at once: exactly one gets 200; the other gets 409 `approval_already_decided`.

Reject, return and invalidated requests never get a run.

### POST `/requests/{request}/resubmit`

The requester, or a member with `integration.manage` (else 403). Body `{"comment"?}` plus an `Idempotency-Key`. Only the latest revision of a `rejected` or `returned` request can be resubmitted, and only while the subject has no live request. Otherwise the answer is 409 `approval_not_resubmittable`. The new revision copies the fingerprint, snapshot and amount, starts `pending` and records `resubmitted_by` and a `resubmitted` event. Response 201 `{"data":{"approval_request_id"}}`.

## Policies

One active policy per workflow and currency. An untaxed amount below `threshold_minor` needs `integration.approve`; an amount at or above it needs `integration.approve_high`. The comparison runs in SQL when the request is created. A currency without a policy blocks the request with `no_policy:<currency>`.

| method + path | permission | body | success | errors |
|---|---|---|---|---|
| GET `/policies?page=&page_size=` | view | — | list of `{id, workflow_key, currency_ref, threshold_minor, active, created_at, updated_at}` | 403 |
| POST `/policies` | manage | `{workflow_key: "wf01_purchase", currency_ref: text 1..64, threshold_minor: int ≥ 0}` | 201 `{"data":{"policy_id"}}` | 403; 422 fields; 422 `currency_ref`: `An active policy already exists for this currency.` |
| POST `/policies/{policy}` | manage | `{threshold_minor?, active?}` | `{"message":"ok"}` | 403, 404; 422 when activating would make a second active policy |

## Intake helpers (C4a)

`modules::approvals::requests` holds the writes the intake job calls inside its own transaction:
- `upsert_revision` creates the next revision (`max + 1`, serialised per subject). A live row with the same fingerprint comes back unchanged. The function adds `no_policy`, `no_requester` and `no_eligible_approver` to the intake's own reasons. "Eligible approver" means an active member other than the requester who is the owner, holds `workspace_admin`, or holds the needed permission (`approve_high` counts for `approve`).
- `unblock` and `set_blocked` re-run those checks.
- `invalidate` refuses a request that already has a run.
