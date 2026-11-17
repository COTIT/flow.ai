# Verzo glue layer — WF-01 Odoo RFQ → Flow approval → NetSuite PO (rev 2)

Status: plan only, no code. Rev 2 answers the 10-point revision request. Unresolved decisions are in §11; each has a recommended default so you can approve them all at once or change any.

---

## 0. Context and product intent

- **Flow** is an AI-native business orchestration platform. **Verzo** is its conversational assistant. Verzo (formerly Genesy) is also an ERP. The plan is for it to act first as the glue between a customer's Odoo and NetSuite, and later to offer its own ERP.
- **Source:** `~/Downloads/verzo_odoo_netsuite_workflow_stories.md`, an unvalidated backlog covering WF-01 to WF-07.
- **WF-01 is the first vertical slice.** It must leave behind reusable *capability* and *execution* contracts, so a second workflow can be added without rewriting the foundation.
- **WF-07 (user-defined workflows) is deferred, not dropped.** Rev 1 wrongly cited the old "no app builder" note; you have overridden it. The contracts in §3 are shaped so that a later builder only *composes* registered capabilities. No builder, DSL or general engine is built now.
- **Genesy's own purchasing is not touched.** It is the M3 work on `claude/erp-m3`, which has an active coordinator. WF-01 never imports `modules/purchasing`.

Already decided with you: build now against fakes copied from the official docs; Odoo 19+ JSON-2 only; the source record is the Odoo RFQ (`purchase.order` in RFQ state); approval UI goes in both Flow and Office; no sandboxes exist yet.

---

## 1. Verified facts this plan rests on

### 1a. Repo (read 2026-11-17)

| Fact | Path |
|---|---|
| `jobs`: leases with a fencing token, `dedupe_key` (one live job per key), `max_attempts`, backoff `min(10·2^(n-1),3600)s` ±20%, `reap` requeues expired leases | `migrations/20261023073000_jobs.up.sql:5-80`, `src/jobs/repo.rs:104-150,284,464-518` |
| Job kind = const + `KINDS` entry + `dispatch` arm. `SCHEDULES` with `Every::Minutes`. `Step::Done/More` | `src/jobs/kinds.rs:21,63,87,170` |
| Enqueue on a duplicate `dedupe_key` returns the existing live job id and does not update its payload | `src/jobs/repo.rs:104-150` |
| `workspace_credentials(workspace_id,path,key_id,sealed)` is XChaCha20 sealed and bound to workspace+path. The path CHECK list is re-issued per migration and mirrored in `PATHS`. Write-only: GET shows the mask `********`. No expiry column | `migrations/20261021151609_credentials.up.sql`, `20261110060000_globadyme.up.sql:31`, `src/crypto.rs:19,66,85`, `src/modules/settings/credentials.rs:5-16` |
| `Outbound` blocks private IPs and metadata addresses at DNS. The only allowlist is a global trusted-hosts list | `src/outbound.rs:1-126` |
| Idempotency, two styles: command replay and `Idempotency-Key` | `src/idempotency.rs`; `accounting/service.rs:101,162`; `invoices/gateway.rs:290,383` |
| Members are `workspace_members(workspace_id,user_id,active)` and `users(id,email,…)` | `migrations/20261020060110_init_auth.up.sql:5,35` |
| Permissions: `Can<P>`, the `permissions!` macro, owner plus team grants | `src/http/workspace.rs:26-125`, `modules/permissions/catalog.rs` |
| Four-eyes approve/reject precedent | `modules/farm_supply_corrections/handlers.rs:127,152` |
| No in-app notification table. `attention` is a read-only computed GET | `modules/attention/mod.rs:1` |
| The Flow approval scene is for *agent tasks*, not business approvals | `apps/flow/components/task/approval-scene.tsx`, `frame/dock.tsx:73` |
| Shared UI | `packages/ui/src/components/{notice,empty-state}.tsx` |
| Crates available: `sha2`, `hmac`, `base64`, `reqwest`, `rust_decimal`, `serde_json`(arbitrary_precision), `uuid`(v7). **No JWT/RSA/EC signing crate** | `apps/api/Cargo.toml` |
| Latest migration on main | `20261115140000_inventory_valuation` |

**Absent:** any Odoo or NetSuite code, connectors, workflow runs, approval policies, cross-system links, correlation ids, and Verzo naming in code.

### 1b. Odoo 19 (raw source `odoo/odoo@19.0/addons/purchase/models/*`, and the JSON-2 doc)

- **API call:** `POST /json/2/<model>/<method>` with `Authorization: bearer <key>`, optional `X-Odoo-Database`, named args only.
  - Each call runs in its own transaction.
  - Errors come back as `{name,message,arguments,context,debug}`.
- **Availability:** Custom plan only. API keys last at most 3 months. The docs state no idempotency guarantee and no rate limits.
- **`purchase.order.state`:** `draft`(RFQ), `sent`(RFQ Sent), `to approve`, `purchase`, `cancel`. There is no `active` field. Only `cancel` orders can be unlinked.
- **Order fields:**
  - `partner_id` (vendor, required), `currency_id`, `company_id`, `user_id` (Buyer)
  - `date_order`, `date_planned`, `origin`, `partner_ref`, `fiscal_position_id`
  - `amount_untaxed`, `amount_tax`, `amount_total`, `order_line`
- **Line fields:**
  - `product_id`, `product_qty` (Float), `product_uom_id`, `price_unit` (Float), `tax_ids`
  - `price_subtotal`, `display_type` (section/subsection/note), `date_planned`
- **Odoo's native approval:**
  - `button_confirm` moves `draft|sent` → `purchase`, or → `to approve` when the company sets `po_double_validation='two_step'` and `amount_total ≥ po_double_validation_amount` (default 5000) and the user is not a purchase manager.
  - `button_approve` → `purchase`.

### 1c. NetSuite REST (Oracle help)

- **Auth:** OAuth 2.0 is preferred; TBA is also supported.
  - **M2M (client credentials):** token URL `https://<accountID>.suitetalk.api.netsuite.com/services/rest/auth/oauth2/v1/token`. The client sends a JWT assertion.
  - **JWT fields:** `alg` ∈ PS256/384/512 or ES256/384/512; `kid` = certificate ID; `iss` = client ID; `scope` = `rest_webservices`; `aud` = the token URL; `exp` < `iat`+60 min.
  - **Token and certificates:** the token lives 3600 s; on expiry the endpoint returns `invalid_grant` and the client restarts the flow. Certificates last at most 2 years, with at most 5 active per integration+entity+role.
- **Role permissions:** "REST Web Services" (Full) and "Log in using Access Tokens" (Full).
- **Write calls:**
  - Create = `POST` → 204 + `Location`.
  - `PATCH`: omitted fields stay unchanged; `id` is read-only.
  - Upsert = `PUT /record/v1/<type>/eid:<externalId>`, which creates the record or **updates** it if present. External IDs allow `[A-Za-z0-9_-]`.
- **Errors and limits:**
  - 429 `CONCURRENCY_LIMIT_EXCEEDED` → retry. Requests time out after 15 min.
  - Errors come back as `{status,o:errorDetails:[{detail,o:errorCode,…}]}`.
- **SuiteQL:** `POST /query/v1/suiteql` + `Prefer: transient`, at most 100k results.
- **Async:** `Prefer: respond-async` supports an "idempotency retry mechanism". The header name is not stated.

### 1d. Explicitly unverified (owned by R1, §8)

- **NetSuite:**
  - purchaseOrder REST status (GA/beta) and its exact field and sublist names (`entity`, `subsidiary`, `currency`, `location`, `item`…).
  - GET by `eid:`.
  - What POST does when an `externalId` already exists.
  - How approval routing / `approvalStatus` behaves on a REST create.
  - Per-tier concurrency numbers.
  - The sandbox account-ID → host mapping.
- **Odoo:**
  - That `write_date` / `id` filtering and ordering work through JSON-2 `search_read` (`write_date` is an ORM magic field, not declared in the purchase files).
  - `res.currency.decimal_places`.
  - Whether a version or health probe endpoint exists.
  - Whether `picking_type_id` exists (it lives in `purchase_stock`).

---

## 2. WF-01 business process and ownership (your decisions: §11 D1–D4)

| Question | Rule (recommended default) |
|---|---|
| Who owns the purchase request? | **Odoo.** An RFQ (`draft`/`sent`) in a mapped Odoo company is the request. Flow only reads it. |
| Who owns the resulting PO? | **NetSuite.** Flow creates exactly one NetSuite purchaseOrder per approved request. |
| Approval authority | **Flow only.** It is the single authority. Odoo's native two-step is not used, because the RFQ is never confirmed in Odoo. NetSuite approval routing is not driven by Flow; what a REST create yields under routing is LIVE gate L4. |
| Duplicate obligation | Duplicates are prevented by contract and detected by Flow. **Contract:** after Flow creates the NetSuite PO, the Odoo RFQ must not be confirmed in Odoo. **Detection:** if Flow sees that source RFQ move to `to approve` or `purchase`, it opens a `duplicate_obligation_risk` incident (visible, never auto-fixed). WF-01 makes no Odoo writes, so the Odoo key needs read access only (D2 offers an optional write-back). |
| Vendor | Odoo `res.partner` id → NetSuite vendor id, via an explicit mapping row. No mapping → **blocked** (`unmapped:vendor`). |
| Product | Odoo `product.product` → NetSuite item, mapping row. Lines with `display_type` set (sections and notes) are ignored. |
| Unit | Odoo `uom.uom` → NetSuite unit, mapping row. If the NetSuite side is unmapped and the line's unit has no mapping → blocked. |
| Currency | Odoo `res.currency` → NetSuite currency, mapping row. The amount is converted to integer minor units using the Odoo currency's `decimal_places` (R1). The threshold policy is per currency. No FX in WF-01. A currency with no policy → blocked. |
| Company / subsidiary | Odoo `res.company` → NetSuite subsidiary, mapping row, required. |
| Location | Optional mapping. Odoo `picking_type_id` → NetSuite location if `purchase_stock` is installed, else a per-subsidiary default location from config. Whether NetSuite requires a location is LIVE gate L3. |
| Taxes | **Not transferred in WF-01** (D3). The NetSuite PO lines carry quantity and rate only; NetSuite computes tax under its own setup. The approval shows the Odoo untaxed amount, tax and total, labelled "NetSuite will calculate tax". The threshold is applied to `amount_total`. |
| Unmappable entity | The request is created with status `blocked`, listing every missing mapping at once. Fixing the mappings re-evaluates the request automatically on the next poll pass. A blocked request never reaches approvers. |
| Systems disagree | Flow never overwrites. If the NetSuite PO already exists and its data differs from the approved snapshot → `awaiting_reconciliation` plus an incident, and a human decides. Flow trusts Odoo for request content and NetSuite for PO content after creation. |

---

## 3. Architecture and module boundaries

```
apps/api/src/
  integrations/            (shared, provider-agnostic; no Odoo/NetSuite words)
    capability.rs          capability keys + Provider trait + typed DTOs
    connections            module: connection CRUD, test, health
    mappings               module: entity mappings
    links.rs               external_links
    cursors.rs             poll cursors
  integrations/providers/
    odoo/                  JSON-2 client + impl PurchaseRequestSource
    netsuite/              OAuth2 M2M + REST client + impl PurchaseOrderSink
  modules/approvals/       generic approval requests/decisions/policies (subject = opaque ref)
  workflows/               shared execution contract (executions + events + runner helpers)
    wf01_purchase/         poll.rs, evaluate.rs, execute.rs, reconcile.rs (WF-01 rules only)
```

**Capability contract (reusable, the future builder composes these).** These are typed Rust traits; there is no plugin system or DSL:

- `PurchaseRequestSource`
  - `list_changed(cursor, page) -> Page<SourceRef+fields>`
  - `read(ids) -> Vec<PurchaseRequest>` (absent ids are reported as gone)
- `PurchaseOrderSink`
  - `find_by_external_id(eid) -> Option<TargetPo>`
  - `create(eid, NewPo) -> CreateOutcome{Created(id)|Ambiguous|Rejected(err)}`
  - `read(id) -> TargetPo`
- `ConnectionProbe`
  - `test() -> Vec<CapabilityCheck{key, ok, error_class}>`

The provider DTOs normalise into neutral structs (`PurchaseRequest`, `NewPo`, `Line`, `Money{minor:i64,currency_ref}`). Odoo and NetSuite field names stay inside `providers/`.

Every client call returns `ErrorClass` = `Transient | Ambiguous | Auth | Permission | NotFound | Validation | Conflict | Unknown`. Mapping tables live in R1-backed `providers/*/errors.rs`, one tested case per documented code.

The capability registry is a static `&[CapabilityKey]` per provider. A connection stores the result of its last probe, so the UI (and later the builder) knows what is available. No dynamic registration.

**Execution contract (generic, rows only, no engine):**

- `workflow_executions`: one row per approved operation, holding status, attempts, correlation, target ref and error class.
- `workflow_events`: append-only audit covering trigger, snapshot, mapping result, proposal, decision, API request summary, API result, verification and incident.
- Helpers: `claim_execution`, `transition(from→to)` (SQL `UPDATE … WHERE status = from`, row-count checked), `record_event`.
- WF-01 code is the only place that knows the step order.

---

## 4. Data model (one reversible migration per child, `YYYYMMDDHHMMSS_*`, after the latest on main at merge time)

All tables have `workspace_id` with composite FKs `(workspace_id,id)`, `uuidv7` ids, CHECK constraints for enums, and `created_at/updated_at`.

**C1 — integrations**
- `integration_connections`
  - Columns: `provider CHECK(odoo|netsuite)`, `label`, `config jsonb` (non-secret, validated per provider), `status CHECK(draft|active|degraded|unauthorized|expired|disconnected)`.
  - Also: `credential_expires_at` (user-entered: Odoo key ≤3 months, NetSuite cert ≤2 years), `last_probe jsonb`, `last_probe_at`.
  - Unique `(workspace_id, provider, label)`.
- Credentials reuse `workspace_credentials`, with new path CHECK entries `integration.odoo.api_key` and `integration.netsuite.private_key`.
  - Paths are keyed per connection by a suffix only if the CHECK allows a pattern. The current CHECK is a fixed list, so R1/C1 verifies whether a `LIKE` pattern is acceptable to the existing `PATHS` code.
  - **Open D6.** The fallback is a dedicated `integration_secrets` table using the same `CredentialKey::seal` with path `integration:<connection_id>:<name>`.
- `integration_pairs`: `(source_connection_id, target_connection_id, workflow_key)`, so WF-01 knows which Odoo feeds which NetSuite. Config: `poll_minutes`, `backfill_days`, `default_location` per subsidiary.
- `integration_mappings`
  - Columns: `connection_pair_id`, `entity CHECK(company|vendor|product|uom|currency|user|location)`, `source_id text`, `target_id text`, `created_by`.
  - Constraints: unique `(pair, entity, source_id)`. `user` maps an external user → `workspace_members.user_id` (target is the uuid; FK checked in the service).
- `integration_poll_cursors`: `(connection_id, stream)` PK, `cursor_ts timestamptz`, `cursor_id bigint`, `updated_at`.
- `external_links`: `correlation_id text` unique per workspace, `source (connection,model,id)`, `target (connection,type,id nullable)`, `state CHECK(pending|linked|conflict)`.

**C2 — approvals (generic)**
- `approval_policies`: `workflow_key`, `currency_ref`, `threshold_minor bigint ≥0`, `approver_permission text`, `active`. Unique `(ws, workflow_key, currency_ref)` where active.
- `approval_requests`
  - Columns: `workflow_key`, `subject_ref text` (opaque, e.g. `odoo:<conn>:purchase.order:<id>`), `fingerprint char(64)`, `snapshot jsonb` (the exact action: values + target system + target connection), `amount_minor`, `currency_ref`, `requester_user_id` (nullable when unmapped → blocked), `status CHECK(blocked|pending|approved|rejected|returned|invalidated)`, `blocked_reasons text[]`, `decided_by`, `decided_at`, `invalidated_reason`.
  - Partial unique `(ws, workflow_key, subject_ref) WHERE status IN ('blocked','pending','approved')`, so there is at most one live request per source.
  - Unique `(ws, workflow_key, subject_ref, fingerprint)`, so one request per source revision.
- `approval_decisions`: append-only (no UPDATE/DELETE, enforced by trigger as in existing append-only tables), holding `request_id`, `actor_user_id`, `decision CHECK(approve|reject|return)`, `comment`, `fingerprint_seen`.

**C2 — workflows (generic execution)**
- `workflow_executions`
  - Columns: `approval_request_id` unique, `workflow_key`, `correlation_id`, `status CHECK(queued|running|blocked|awaiting_reconciliation|succeeded|failed)`, `attempts`, `error_class`, `error_detail` (≤1000), `target_ref`, `job_id`.
  - CHECK: succeeded ⇒ target_ref is not null.
- `workflow_events`: append-only, holding `execution_id` or `approval_request_id`, `kind`, `payload jsonb` (≤64KB, secrets never stored), `actor` (user|system).
- `workflow_incidents`: `kind CHECK(duplicate_obligation_risk|target_conflict|source_gone_after_execution|connection_unhealthy)`, `subject_ref`, `status CHECK(open|resolved)`, `resolved_by`, `note`.

Money: integer minor units. Odoo Floats arrive as JSON numbers; `serde_json` arbitrary_precision → `rust_decimal`, then scaled by `decimal_places`. Values that do not round exactly are refused (blocked `amount_not_exact`), never rounded silently. Threshold comparison happens in SQL.

---

## 5. State machines

### 5a. Approval request
```
            (mappings/requester/policy missing)
  new ──► blocked ──(re-evaluated OK)──► pending
  new ──────────────────────────────────► pending
  pending ──approve──► approved
  pending ──reject───► rejected        (terminal)
  pending ──return───► returned        (terminal for this fingerprint; a new fingerprint creates a new request)
  pending|blocked ──source changed/cancelled/gone──► invalidated (terminal)
  approved ──source changed before execution claimed──► invalidated (terminal)
```
- **Concurrency.** Each decision runs in one transaction: `UPDATE approval_requests SET status=… WHERE id=$1 AND status='pending' AND fingerprint=$fp_seen`, then an INSERT of the decision.
  - If 0 rows change → 409 `approval_already_decided` (two approvers acting at once: one wins).
  - The decide call carries the fingerprint the approver saw, so a stale screen cannot approve a changed RFQ.
- **Four-eyes.** The approver must:
  - not be the requester (the mapped Odoo buyer), nor the connection's creator if you choose D5;
  - be an active member;
  - hold `approver_permission`;
  - be in the same workspace.
  - The owner does not bypass four-eyes.
- **No eligible approver.** If no active member other than the requester holds the permission, the request goes to `blocked` with `no_eligible_approver`, visible to members holding `integration.manage`.
- **Below threshold.** D4 default: still requires one approval, with no auto-approve in WF-01. The threshold only decides which permission is required (`integration.approve` vs `integration.approve_high`).
- **Assignment.** A pool, not a named person: any eligible member can decide. No reassignment is needed. Reject or return needs a comment.

### 5b. Execution (exists only once approval = approved)
```
  queued ──claim──► running
  running ──created + verified──► succeeded
  running ──Transient (pre-send, or 429)──► queued   (attempts < max)
  running ──Ambiguous (timeout/5xx/conn reset after send)──► awaiting_reconciliation
  running ──Auth/Permission/expired creds/connection down──► blocked
  running ──Validation/NotFound mapping──► blocked
  running ──source fingerprint differs──► failed (source_changed) + approval→invalidated
  running ──target exists with different data──► awaiting_reconciliation + incident target_conflict
  awaiting_reconciliation ──reconcile finds PO matching snapshot──► succeeded
  awaiting_reconciliation ──reconcile proves absent──► queued
  awaiting_reconciliation ──reconcile finds mismatch / user decides──► failed | succeeded(user accepts link)
  blocked ──user "retry after fix"──► queued   (all pre-checks rerun)
  queued|blocked ──attempts ≥ max──► failed (terminal)
```
- **Invariants:**
  - `approved` never implies `succeeded`; the UI shows both.
  - `rejected`, `returned` and `invalidated` have no execution row, and the CHECK + service refuse to create one.
- **Claim transaction.** `SELECT approval FOR UPDATE`, require `status='approved'`, insert the execution (unique `approval_request_id`), then enqueue the job with `dedupe_key = execution_id`. Invalidation takes the same row lock and is refused once an execution row exists. After that point, source changes produce incidents, not invalidation.

---

## 6. Polling, execution, retry, reconciliation

### 6a. Poll (`wf01_poll`, per active pair, `SCHEDULES` every `poll_minutes`, `concurrency_key = pair id`)
- **Eligibility:** `state in ('draft','sent')`, `company_id` in the mapped companies.
- **Scope on first run:** `cursor = activation_time − backfill_days` (D7 default 0 = only RFQs changed after activation).
- **Query:** `search_read` with the domain `['|', ('write_date','>',ts), '&', ('write_date','=',ts), ('id','>',last_id)]`, ordered `write_date asc, id asc`, `limit` 100. It reads the business fields in §1b plus `state` and `write_date`. Validity is R1 item O1.
- **Overlap.** Each pass actually starts from `cursor_ts − 120s`, because an Odoo commit can become visible after a later write_date. The duplicates this produces are harmless (fingerprint unique).
- **Per record, in one Postgres transaction per page:**
  1. compute the fingerprint;
  2. upsert the request (new fingerprint → invalidate the old live request with `source_changed`, then create a new one);
  3. append the event;
  4. advance the cursor to the page's last `(write_date,id)`.
- **Crash recovery.** A crash mid-page rolls back the whole page, the cursor does not move, and the page is re-read. A job lease expiry is handled by `reap`.
- **Leaving eligibility.** RFQs that leave the eligible states are not returned by this query. That is handled by the sweep.
- **Sweep (`wf01_sweep`, same schedule).**
  - For every live request (blocked/pending/approved without an execution): `read(ids)`.
    - Gone, cancelled or no longer `draft|sent` → `invalidated` with a reason.
    - Fingerprint differs → invalidate and create a new request.
    - Blocked requests are re-evaluated against the current mappings.
  - For succeeded executions within the last N days (config, default 30): `read` the source. `to approve`/`purchase` → `duplicate_obligation_risk` incident; gone → `source_gone_after_execution` incident.
- **Fingerprint.** SHA-256 over canonical JSON (sorted keys, decimals as strings) of:
  - `partner_id, currency_id, company_id, user_id, picking_type_id?`
  - each non-display line `(id, product_id, product_qty, product_uom_id, price_unit, tax_ids sorted, date_planned)` sorted by line id
  - `amount_untaxed, amount_tax, amount_total`
  - `write_date` is **excluded**: a material change matters, a touch does not.
- **Changes while pending** → invalidated, then a new request. **Changes just before execution** → caught by the execute pre-check (6b step 1).

### 6b. Execute (`wf01_execute`, `dedupe_key=execution_id`, `max_attempts` 5)
1. **Re-read the source** `read([id])`. It must still be `draft|sent`, and its fingerprint must equal the approved fingerprint. Otherwise → `failed(source_changed)` + approval `invalidated`.
2. **Revalidate.**
   - Mappings: every mapped id is unchanged since approval; they are snapshotted, and any difference → `blocked(mapping_changed)`.
   - Approver: still an active member with the permission at execution time (D8 default: yes, recheck) → else `blocked(approver_revoked)`.
   - Connections: both are `active`.
3. **Correlation / externalId** = `"vz-" + hex(sha256(workspace_id ‖ workflow_key ‖ pair_id ‖ subject_ref ‖ approved_fingerprint))[0..32]`. It is deterministic and workspace-scoped, and stable for this one approved operation. A new revision gets a new id.
4. **Existence check:** `find_by_external_id(eid)`.
   - Found and it matches the snapshot (vendor, subsidiary, currency, lines qty/rate/item) → `succeeded` (link it). This covers a lost response on a prior attempt.
   - Found and it differs → `awaiting_reconciliation` + `target_conflict`. **Never PATCH or PUT.**
5. **Create** with **POST** and `externalId` in the body (not PUT upsert: upsert updates existing records, which is a documented overwrite risk).
   - `Created(id)` → step 6.
   - `Transient` before any bytes were sent, or 429 → requeue.
   - `Ambiguous` → `awaiting_reconciliation` and a `wf01_reconcile` job.
   - `Validation/Auth/Permission` → `blocked`, holding the NetSuite `o:errorCode` and a readable message.
6. **Verify:** `read(id)` and compare with the snapshot. Match → `succeeded`, `external_links.state=linked`, event. Mismatch → `awaiting_reconciliation` + incident.
7. **Notify:** there is no notification primitive (fact). WF-01 shows the outcome in the requester's Flow/Office approvals list ("My requests") and records an event. Email to the requester via `messages::service::send` is D9, default off.

### 6c. Reconcile (`wf01_reconcile`, backoff via job retry, `max_attempts` 10)
- Runs `find_by_external_id(eid)`.
  - Found + match → `succeeded`.
  - Found + mismatch → stays, with an incident.
  - Absent → it is safe to create, but only if the last attempt is older than the NetSuite 15-min request timeout (documented), so no in-flight request can still land. Then → `queued`.
  - Exhausted → stays `awaiting_reconciliation`, visible, and a human chooses: "Mark failed" or "Link existing PO id" (the PO is read and compared first).

### 6d. Retry and failure classes
| Class | Examples | Action |
|---|---|---|
| Transient | 429 `CONCURRENCY_LIMIT_EXCEEDED`, connect refused before send, DNS | job retry with backoff, bounded by `max_attempts` |
| Ambiguous | timeout or 5xx after the request was sent, connection reset mid-response | reconcile, never blind retry |
| Auth | NetSuite `invalid_grant`, 401 `INVALID_LOGIN`, Odoo 401 "Invalid apikey" | token refresh once (NetSuite), else connection → `unauthorized`, execution → `blocked` |
| Permission | 403 / insufficient role | connection `degraded`, execution `blocked` |
| Validation | 400 `INVALID_CONTENT`, `INVALID_REQUEST` | `blocked` with detail; user fixes the mapping |
| NotFound | 404 `NONEXISTENT_ID` on a mapped id | `blocked(mapping_stale)` |

**Disconnected/expired connection.** Polls pause (the job finishes with `skipped`), executions → `blocked(connection_*)`. `credential_expires_at` within 14 days → `degraded` + a banner; past → `expired`.

---

## 7. Connection configuration

- **Odoo config:**
  - `base_url` must be https, pass the `Outbound` guard, and have no path.
  - Optional `database` (sent as `X-Odoo-Database`).
  - `api_key` (sealed), `credential_expires_at`.
  - **Probe:** `search_read purchase.order limit 1`, `read res.company`, plus whatever R1 confirms for the version (O3).
- **NetSuite config:**
  - `rest_domain` is copied by the user from Company Information › Company URLs (a documented location), stored as the host. There is no derivation from the account id (the sandbox rule is unverified).
  - `environment CHECK(production|sandbox)` is a label only, warning when it mismatches the domain.
  - Also `client_id` (iss), `certificate_id` (kid), `alg`, `private_key` (sealed, PEM), `credential_expires_at`.
  - **Probe:** fetch a token, then one metadata or SuiteQL read (R1 picks a documented read that needs only the stated permissions).
- **Hosts:** `Outbound` per request. No new trusted hosts; ERP hosts are public.
- **Tokens:** NetSuite access tokens are cached in process memory per connection until `exp−60s`. They are never persisted or logged.
- **Secrets:** write-only. Responses show `********`. Events and logs store a request summary (method, path, status, error code), never headers or bodies containing secrets.
- **Isolation:** every connection, mapping, request and execution is workspace-scoped, with a test proving another workspace gets 404.

---

## 8. R1 fact pack (blocks adapter code that depends on it)

Output: `docs/integrations/facts/odoo-19.md` and `netsuite-rest.md`. Each operation records:
- the official URL;
- versions;
- the request/response contract;
- auth and permissions;
- required business fields;
- errors and retry behaviour;
- confirmed limits;
- **remaining unverified assumptions**.

| ID | Item | If not documented |
|---|---|---|
| O1 | JSON-2 `search_read` domain/order/limit on `write_date`,`id` | adapter waits; plan falls back to `search` + `read` only if documented |
| O2 | `res.currency.decimal_places` field | blocked; no money conversion without it |
| O3 | version/health probe | probe uses capability calls only |
| O4 | `picking_type_id` location (purchase_stock) | location uses the default-location config only |
| N1 | purchaseOrder REST support + field/sublist names (REST API Browser / metadata catalog) | **C3-NetSuite blocked** |
| N2 | GET by `eid:` or SuiteQL `externalid` lookup | existence check uses the documented `q=externalId IS "…"` form |
| N3 | POST with an existing externalId behaviour | treated as unknown: the existence check always runs first; LIVE L2 |
| N4 | approvalStatus / approval routing on REST create | Flow sets no approval fields; LIVE L4 |
| N5 | concurrency per tier | 429 handling only |
| N6 | 400/403 codes for role-permission failures | classified `Permission` only by HTTP 403 |

**Fakes:** in-process axum servers in `apps/api/tests/`. Each fixture response cites its fact-pack line. They also include **adversarial** responses: 429, timeouts after the handler commits, 5xx after create, malformed JSON, the wrong `Location`, an existing eid with different data, `invalid_grant`, and Odoo 401/403. The fake NetSuite records creates so tests can assert "exactly one PO".

**Wording rule:** tests and docs say "verified against documented contract", never "integrated with Odoo/NetSuite".

**LIVE gates (blocked until sandboxes, tracked in `docs/integrations/README.md`):**
- L1 Odoo end-to-end read on a real Custom-plan v19 instance.
- L2 NetSuite create/duplicate externalId behaviour.
- L3 required purchaseOrder fields (location/subsidiary) under a real account's features.
- L4 approval routing.
- L5 M2M token + permissions on a least-privilege role.
- L6 currency/vendor restrictions.

---

## 9. Acceptance criteria (behavioural; every row is a named test)

**API, `#[sqlx::test]` + fakes, asserting DB state and fake-server call log:**
1. A new RFQ → exactly 1 `pending` request with the exact snapshot. A second poll → still 1 (duplicate poll).
2. The same `wf01_execute` enqueued twice / executed twice → 1 NetSuite create call and 1 PO.
3. The RFQ is modified after approval and before execution → the execution is `failed(source_changed)`, the approval `invalidated`, a new pending request exists, and 0 creates.
4. The RFQ is modified while pending → the old request is `invalidated` and a new one exists. Approving the stale fingerprint → 409.
5. Two approvers decide at the same moment (two connections, barrier) → one 200, one 409, one decision row.
6. The requester tries to approve their own request → 403. The owner who is also the requester → 403.
7. No eligible approver → `blocked(no_eligible_approver)`.
8. Rejected/returned/invalidated → no execution row, and the job is never enqueued.
9. Missing vendor or product mapping → `blocked` listing all missing entities. Adding the mapping → the next sweep moves it to `pending`.
10. Currency without a policy → blocked. Subsidiary unmapped → blocked. Amount not exact in minor units → blocked.
11. The create succeeds at the fake but the response times out → `awaiting_reconciliation`. Reconcile finds it → `succeeded`, with 1 PO total.
12. An existing NetSuite PO with the same eid and different lines → `awaiting_reconciliation` + `target_conflict` incident, and 0 PATCH/PUT calls.
13. NetSuite `invalid_grant` → one token refresh then success. A persistent 401 → connection `unauthorized`, execution `blocked`.
14. 403 permission → `blocked`, connection `degraded`, readable message (no raw codes on screen).
15. 429 → retried with backoff, succeeds, 1 PO. 429 forever → `failed` after `max_attempts`, visible.
16. Worker crash: the lease expires mid-execute → `reap` requeues. The second run's existence check finds the PO → `succeeded`, 1 PO.
17. A crash mid-poll page → the cursor is unchanged and there are no duplicate requests after rerun.
18. Equal `write_date` across page boundaries → no RFQ is skipped (fake returns 250 RFQs with 3 identical timestamps).
19. The source is cancelled in Odoo while pending → `invalidated`. Confirmed in Odoo after success → `duplicate_obligation_risk` incident.
20. Credential expiry in the past → connection `expired`, poll skipped, executions `blocked`.
21. Cross-workspace: another workspace gets 404 on connections, mappings, requests, decisions, executions and incidents, and its member cannot approve.
22. Secrets: GET never returns them, and the events table never contains a key or a token (grep assertion on the payloads).
23. Partial failure is visible: list endpoints expose approval status + execution status + incident separately.

**Playwright (mocked + stack, desktop + 375px, no console errors / failed requests):**
- An approver approves in Flow, and Office shows the same request as approved/succeeded.
- A blocked request shows the missing mappings and a link to the mapping screen.
- A stale-approve conflict shows a readable message.
- The awaiting-reconciliation state is shown with its actions.
- Connection setup saves, secrets are masked, and the probe result is shown.

---

## 10. Child sessions, contracts first, waves

**Wave 0 (parent, before any parallel code).** The parent writes `docs/integrations/contracts.md`, which freezes:
- the table/column names (§4);
- the state enums (§5);
- the Rust trait signatures and neutral DTOs (§3);
- the `ErrorClass` set;
- the REST routes + JSON shapes for approvals, executions, connections and mappings.

The children then build against frozen names. A change to a contract goes through the parent.

| Child | Scope | Depends on | Model |
|---|---|---|---|
| **R1** facts pack | §8 docs only | — | Opus 5.5 medium |
| **C1** integrations core | connections, secrets (D6), pairs, mappings, cursors, links, probe framework (trait + fake probe), permissions `integration.view/manage/approve/approve_high`, routes, tests 20-22 for its tables | W0 | Opus 5.5 medium |
| **C2** approvals + execution contract | approval_policies/requests/decisions, workflow_executions/events/incidents, transition helpers, decide route with fingerprint + four-eyes + no-eligible-approver, tests 4-8, 21, 23 | W0 | Opus 5.5 high (concurrency) |
| **C3a** Odoo adapter | JSON-2 client, `PurchaseRequestSource`, `ConnectionProbe`, error map, fakes (normal + adversarial) | R1 (O1-O4), C1 | Opus 5.5 medium |
| **C3b** NetSuite adapter | M2M JWT + token cache, `PurchaseOrderSink`, probe, error map, fakes | R1 (N1-N6), C1, **D10 crate approval** | Opus 5.5 high |
| **C4a** WF-01 intake | poll + sweep + fingerprint + evaluate (mapping/policy/requester) → approval requests; tests 1, 3-4, 9-10, 17-19 | C1, C2, C3a | Opus 5.5 high |
| **C4b** WF-01 execute + reconcile | claim, pre-checks, existence check, POST, verify, reconcile, failure classes; tests 2, 11-16, 20 | C2, C3a, C3b, C4a's evaluate fn | Fable 5.2 medium (ambiguity/crash correctness) |
| **C5** UI | shared `ApprovalList/ApprovalCard/ExecutionStatus` in `packages/ui`, Flow + Office surfaces, Office connection + mapping settings, Playwright | W0 contracts (builds on the mocked API first), C1+C2 merged for stack e2e | Opus 5.5 medium |

**Waves:**
- {R1, C1, C2, C5-mocked}
- then {C3a, C3b}
- then C4a
- then C4b
- then C5-stack
- then the parent gate

Children use their own worktrees (`use_worktree: true`), plan first and wait for parent approval, and commit locally. The parent merges into `claude/verzo-odoo-netsuite-workflow-e3a751`.

**M3 protection.**
- No edits under `modules/purchasing`.
- The shared hot files (`permissions/catalog.rs`, `jobs/kinds.rs`, migrations, generated contracts, `operation-policy.json`) are only touched by C1 (permissions) and C4a/b (job kinds).
- After M3 merges to main, the parent rebases, renumbers migrations later than M3's if needed (not yet applied anywhere), regenerates contracts and reruns the full gate.

---

## 11. Open decisions (recommended default first)

- **D1 Ownership.** Odoo owns the request, NetSuite owns the PO, Flow is the sole approver. *(Alt: NetSuite-native approval as the authority. That needs L4 verified first.)*
- **D2 Duplicate obligation.** Detect-and-incident only, with a read-only Odoo key. *(Alt: after success, Flow cancels the Odoo RFQ via `button_cancel`. That needs an Odoo write key and is a second side effect.)*
- **D3 Taxes.** Not transferred; NetSuite calculates. *(Alt: an Odoo tax → NetSuite tax code mapping. That needs SuiteTax facts, which are unverified.)*
- **D4 Below threshold.** Still 1 approval; the threshold selects `approve` vs `approve_high`. *(Alt: auto-approve below the threshold.)*
- **D5 Four-eyes scope.** Approver ≠ mapped requester. *(Alt: also ≠ the member who configured the connection.)*
- **D6 Secret storage.** Extend `workspace_credentials` if `PATHS` can accept per-connection paths; else a dedicated `integration_secrets` table with the same `CredentialKey`.
- **D7 Backfill.** 0 days (only changes after activation). *(Alt: N days.)*
- **D8 Approver revoked before execution.** Block execution. *(Alt: an approval stands once given.)*
- **D9 Requester notification.** In-app status only; email is off.
- **D10 New crate (needs your approval).** NetSuite M2M requires PS256/ES256 JWT signing, and none is in the tree. Proposal: `jsonwebtoken` (supports PS256/ES256), justified because hand-rolling RSA-PSS/ECDSA is a security risk. *(Alt: NetSuite TBA (OAuth 1.0) via the existing `hmac` crate. The docs say OAuth 2.0 is preferred, and the TBA signature method is unverified.)*

---

## 12. Non-goals (WF-01)

- **Workflow builder (WF-07), natural-language workflow authoring, DSL, generic step engine, versioned workflow definitions.** Deferred. The contracts in §3/§4 keep the door open.
- **WF-02 to WF-06.** Not started. They will reuse the connections, mappings, links, executions and events.
- No Odoo writes (unless D2 alt), no NetSuite PATCH/PUT, no FX conversion, no tax mapping, no NetSuite → Odoo sync, no inbound webhooks (polling only; Odoo webhook payloads are undocumented), no XML-RPC.
- No changes to genesy purchasing, stock or invoices. No Verzo renaming of code or apps.
- No claim of real-world ERP integration until the LIVE gates pass.

---

## 13. Verification and cleanup

- **Child:** its own tests + `cargo fmt`, `clippy -D warnings`, `cargo sqlx prepare --check`, via `~/.genesy/heavy.sh` with the shared `CARGO_TARGET_DIR`.
- **Parent, on the merged tip, locally before push:**
  - full `cargo test`;
  - contract regen clean;
  - Playwright mocked + stack, desktop + 375px;
  - then push, one PR, CI green → merge (standing go).
- **Docs:** `docs/api/integrations.md`, `docs/api/approvals.md`, CHANGELOG, `docs/integrations/{README,contracts,facts/*}.md`.
- **Cleanup:** after each child merges, stop its processes, remove its worktree and target/caches, and prune Docker build cache (keep `genesy-dev-pg` 54320). No new containers.

---

# REV 3 AMENDMENTS (these replace any conflicting text above)

## R3-§2 Duplicate obligation (replaces the "Duplicate obligation" row and D2)

**Fact** (odoo 19.0 `purchase_order.py`): `button_confirm` moves an RFQ to `to approve` unless `_approval_allowed()` returns true. It returns true when any of these holds:
- `po_double_validation == 'one_step'`
- `amount_total < po_double_validation_amount`
- the user is in `purchase.group_purchase_manager`

**Prevention:** a customer Odoo configuration control, which Flow verifies with read-only access.
- **Required on every mapped company:** `po_double_validation='two_step'` and `po_double_validation_amount=0`. With these settings, no non-manager can turn an RFQ into a PO in Odoo.
- **Required process:** Odoo purchase managers do not approve `to approve` orders in Flow-governed companies. This is a customer process; Flow cannot enforce it.
- **Flow checks the settings:** it reads the `res.company` fields at pair activation and on every poll.
  - Settings not met → the pair cannot activate.
  - Settings drift later → the pair is `paused`, no new executions start, and an `odoo_control_drift` incident opens.
- **Remaining risk is detected only, not prevented:** a manager approving in Odoo, or someone creating a PO by hand in NetSuite. The sweep raises a `duplicate_obligation_risk` incident. The docs and UI say: "Prevented for non-managers by Odoo config. Manager actions are detected, not prevented."
- **Alternative (D2b):** call Odoo `button_cancel` before the NetSuite write. This needs an Odoo write key, so it is not in WF-01.
- **New R1 fact check (O5):** `res.company.po_double_validation` and `po_double_validation_amount` must be readable through JSON-2 with the API-key user's rights.

## R3-§5b Execution ownership and at-most-once remote write (replaces §5b, §6b steps 3-5, and §6c)

**New columns on `workflow_executions`:**
- `phase CHECK(prechecking|write_intent|write_sent|verifying|done)`
- `owner_token uuid`
- `write_intent_at`
- `remote_attempts smallint`
- `manual_retry_grants smallint`
- `CHECK(remote_attempts <= 1 + manual_retry_grants)`

**Statuses:** `queued`, `running`, `blocked`, `awaiting_reconciliation`, `cancelled`, `succeeded`, `succeeded_with_variance`, `resolved_manual`, `failed`.

**Claim (one transaction):**
```sql
UPDATE workflow_executions
SET status = 'running', owner_token = $job_lock_token
WHERE id = $1 AND status = 'queued'
```
If 0 rows change, another worker already owns the execution, and this job finishes without doing anything.

**Fencing:** every later state write ends with `WHERE id = $1 AND owner_token = $tok`. A stale worker whose lease expired therefore cannot change state; its HTTP result is only recorded as an event.

**Write intent (own transaction, committed BEFORE the HTTP call):**
```sql
UPDATE workflow_executions
SET phase = 'write_intent', remote_attempts = remote_attempts + 1, write_intent_at = now()
WHERE id = $1 AND owner_token = $tok AND phase = 'prechecking'
  AND remote_attempts < 1 + manual_retry_grants
```
If 0 rows change, no POST is sent.

**Rule W1:** once `phase >= write_intent`, the executor never POSTs again. Any re-entry (retry, reap, or a duplicate job) goes straight to reconciliation. The maximum number of POSTs per execution is 1 plus the number of human-granted retries.

**Supporting rules:**
- The pre-write existence check stays as a cheap guard, but correctness does not depend on it (check-then-POST is not atomic).
- An externalId conflict response from NetSuite, if NetSuite returns one, is classed as `Conflict` and goes to reconciliation, never to a retry. Whether NetSuite returns such a response is unverified (N3).

**How each concurrency source is covered:**

| Source | Covered by |
|---|---|
| Duplicate scheduler slot | job dedupe + claim CAS |
| Two workers | claim CAS |
| Lease expiry mid-POST | rule W1 |
| Stale worker returns | fencing token |
| Concurrent HTTP | write-intent CAS: only one POST |

## R3-§5 Source change by phase (replaces the conflicting invalidation text)

| Phase when the change is detected | Effect |
|---|---|
| Approved, no execution row | Approval → `invalidated`; a new request revision is created. |
| Execution `queued` or `prechecking`, no write intent yet | Only the owning executor's pre-check decides: execution → `cancelled(source_changed)`, approval → `invalidated`. The sweep never changes executions. A CHECK constraint allows `cancelled` only when `remote_attempts=0`. |
| `write_intent` or `write_sent` (write in progress) | Approval stays `approved`. Incident `source_changed_during_write`. Reconciliation decides the outcome. Never `cancelled`. |
| `succeeded*` | Incident `source_changed_after_execution`. The NetSuite PO is not touched. |
| `awaiting_reconciliation` | Same as write in progress. |

**Invariants:**
- **I1:** if the approval is `invalidated`, no execution for it has `remote_attempts>0`.
- **I2:** `cancelled` implies `remote_attempts=0`.
- **I3:** `succeeded` implies a `verify_match` event exists (the target was read back and matched the snapshot).
- **I4:** a request that is rejected, returned, or invalidated before claim has no execution row.

## R3-§6c Reconciliation (replaces §6c)

- **Evidence that the PO exists:** a lookup by externalId returns a record. Flow reads it and compares it with the snapshot.
  - Match → `succeeded`.
  - Mismatch → incident, and the execution stays `awaiting_reconciliation`.
- **Evidence that the PO does not exist is never accepted automatically in WF-01.** Whether an externalId lookup is complete and consistent is unverified (N2, L2).
  - A not-found result keeps the execution `awaiting_reconciliation` and re-checks with backoff (max 10 attempts).
  - After that it stays open for investigation. No timeout-based assumption is made.
- **Human resolutions:** these require permission `integration.reconcile`, and the person must not be the requester. A reason is required. Each one is written to an append-only `workflow_resolutions` table recording the actor, an evidence note, and the NetSuite id if there is one.
  - **"Link existing PO":** Flow reads the PO and compares it.
    - Match → `succeeded` (verified).
    - Mismatch → `resolved_manual(linked_with_differences)`. This is never shown as verified.
  - **"Confirmed absent, allow one more attempt":** `manual_retry_grants+1`, status → `queued`, same externalId.
  - **"Close, no PO":** `resolved_manual(closed_no_po)`.

## R3-§2b Financial consistency (replaces the Taxes and Currency rows, and D3)

**Normalisation:**
- Odoo floats arrive as arbitrary-precision JSON and are parsed into `rust_decimal`.
- Money is converted to minor units using the Odoo currency's `decimal_places` (O2).
- Quantities are compared as exact decimals.

**Pre-write checks.** Any failure → `blocked` before write intent, so nothing is sent.
- **P1:** for every non-display line, `round_half_away(qty × price_unit)` at the currency's decimals must equal the line's `price_subtotal` in minor units. Otherwise → `blocked(rate_precision)` (this happens when Odoo carries more price decimals than the currency allows).
- **P2:** the sum of line amounts in minor units must equal `amount_untaxed` exactly.
- **P3:** values are sent unchanged. The NetSuite rate is Odoo `price_unit` and the quantity is Odoo `product_qty`. Vendor, currency and subsidiary come from the mappings.

**Post-write verification.** The PO already exists at this point, so these checks cannot stop it; they route it to review.
- **V1:** NetSuite line quantity, rate, item, currency and subsidiary must equal the values Flow sent. Otherwise → `awaiting_reconciliation` + `target_conflict`.
- **V2:** the NetSuite untaxed total (field name per N1) must equal Odoo `amount_untaxed` in minor units. Otherwise → same as V1.
- **V3 (tax):** NetSuite calculates tax. If the difference from Odoo `amount_tax` exceeds the workspace tolerance (default **0**) → `succeeded_with_variance` plus a review exception, not `succeeded`.
  - The review needs `integration.reconcile`, must be done by someone other than the requester, and needs a reason.
  - Accept → `resolved_manual(variance_accepted)`.
  - Reject → the incident stays open for manual correction in NetSuite.

**What approvers see and what the threshold uses:** `amount_untaxed`, which is exact-matched before the write. Odoo tax and total are shown and labelled "Odoo estimate; NetSuite calculates tax".

**NetSuite rules (unverified, L3/L6):** NetSuite enforces its own subsidiary, currency and vendor rules. A NetSuite validation error → `blocked` with its message, never retried.

## R3-§4 Uniqueness (replaces the approval_requests unique constraints)

- **Kept:** the partial unique index on `(ws, workflow_key, subject_ref) WHERE status IN ('blocked','pending','approved')`.
- **Changed:** the unique constraint on fingerprint is dropped. A `revision int` column is added, counting up per subject. Rows are never deleted, so the full history stays.
- **What a poll does with an unchanged fingerprint:**
  - Latest request was **rejected or returned** → no new request; an event `skipped_previously_decided` is recorded. The requester can explicitly resubmit from Flow, which creates a new revision with the same fingerprint and is audited.
  - Latest request was **invalidated** → a new revision is allowed.

## R3-§6a Safety net for missed changes (adds to §6a)

- The incremental cursor makes no assumption about the order in which Odoo commits writes.
- **New job `wf01_full_scan`** (hourly, configurable):
  1. `search` all eligible RFQ ids per company, paged by `id asc`.
  2. `read` the fingerprint fields.
  3. Compare with the latest known fingerprints, then create or invalidate requests exactly as the poll would.
- **Guarantee:** any RFQ change is detected within one full-scan period, even if the incremental cursor skipped it.
- **Test:** a late-committed write carrying an older `write_date`.

## R3-§C5 UI separation

- The new components live in `packages/ui/src/components/business-approvals/*` (`PurchaseApprovalCard`, `ExecutionStatus`).
- `apps/flow/components/task/approval-scene.tsx`, which handles agent-task approvals, is not reused or renamed.
- The new approvals get their own label ("Purchase approval"), their own route (`/approvals/purchases`) and their own API.
- A Playwright test asserts the agent-task approval scene behaves exactly as before.

## R3-§10 Child sessions (changes only)

- **C4b (execute + reconcile):** Fable 5.2, high effort (the strongest configuration available).
- Other children are unchanged.

## R3-D10 Crypto dependency (replaces D10)

**Fact:** `ring` is already compiled into the API through `rustls` with `features = ["ring"]` (`apps/api/Cargo.toml:25`). ring provides ES256 (ECDSA P-256, fixed format) and RSA-PSS signing.

**Proposal:**
- Make `ring` a **direct** dependency on the same version line, so no new crate enters the build.
- Sign the ES256 JWT with the existing `base64` + `serde_json` crates: about 40 lines, plus a test against a known-key vector.
- Customers generate an EC P-256 certificate; NetSuite documents EC 256 as allowed.
- `jsonwebtoken` is NOT added.

This is still a `Cargo.toml` change, so it needs your approval under the CLAUDE.md dependency rule.

## R3-§9 Acceptance tests (added to §9; tests 11 and 16 are replaced)

All of these are adversarial and concurrent. Each test asserts both the database state and the fake NetSuite's log of create calls.

- **T11′:** the create commits at the fake, then the response times out.
  - Result: `awaiting_reconciliation`, 1 POST.
  - The reconcile lookup finds the PO → `succeeded`, 1 PO in total.
- **T16′:** the worker's lease expires while its POST is in flight (the fake holds the response), and `reap` hands the job to a second worker.
  - The second worker sees `phase=write_intent` and does not POST.
  - The first worker comes back; its fenced update is rejected.
  - Result: 1 POST in total.
- **T24:** 8 workers claim the same execution at once (barrier).
  - Exactly 1 claim succeeds, 1 POST is sent, and 7 jobs finish without doing anything.
- **T25:** the scheduler fires the same slot twice and `wf01_execute` is enqueued 3 times.
  - Result: 1 execution row, 1 POST.
- **T26:** two workers race on the write-intent CAS, with forced interleaving.
  - Exactly one gets the row, so there is 1 POST.
- **T27:** the eid lookup returns not-found after an ambiguous POST.
  - The execution stays `awaiting_reconciliation` through all 10 re-checks, with 0 extra POSTs.
  - A manual "allow one more attempt" by a reconciler who is not the requester sends exactly 1 more POST with the same eid, and the action is audited.
  - The same action by the requester → 403.
- **T28:** "Link existing PO" on a mismatching PO → `resolved_manual(linked_with_differences)`, never `succeeded`.
- **T29:** the source changes in each phase (approved-unclaimed, prechecking, write_intent, write_sent, succeeded, awaiting_reconciliation) produce exactly the outcomes in the R3-§5 table. A CHECK violation is raised if code tries to cancel an execution with `remote_attempts>0`.
- **T30:** P1 rate precision. Price 0.333 × qty 3 in a 2-decimal currency → `blocked(rate_precision)`, 0 POSTs.
- **T31:** NetSuite tax differs from Odoo by 1 minor unit at tolerance 0 → `succeeded_with_variance` and an exception.
  - Accepting the variance → `resolved_manual(variance_accepted)`.
  - The status is never plain `succeeded`.
- **T32:** NetSuite returns a line rate different from the one sent → `awaiting_reconciliation` + `target_conflict`.
- **T33:** Odoo control drift. `po_double_validation` changes to `one_step` → the pair is `paused`, no claims start, and an incident opens. An unmet control at activation → activation is refused.
- **T34:** an RFQ is confirmed in Odoo after the NetSuite PO was created → `duplicate_obligation_risk` incident.
- **T35:** a full scan catches a change whose `write_date` is older than the cursor (a late commit) → the request is invalidated and re-created within 1 scan.
- **T36:** after a reject with the same fingerprint, a re-poll creates no new request. An explicit resubmit creates revision+1. History keeps every row.
- **T37 (Playwright):** the agent-task approval scene in Flow behaves unchanged. The purchase approval appears only at `/approvals/purchases`.
- **T38:** ES256 JWT. The signature verifies against a fixed test key, and the header and claims match the documented fields.

## R3 Readiness verdict

- **Ready to start:** Wave 0 (frozen contracts doc), R1, C1, C2 and C5-mocked. No external access is needed, and none of them depends on unverified API behaviour.
- **Gated:**
  - C3a: R1 items O1, O2 and O5.
  - C3b: R1 item N1 and approval of D10.
  - C4a and C4b: C3.
  - Every real-world claim: LIVE L1–L6, which need sandboxes.
- **Remaining product decisions:** D2-control (is the customer Odoo config + process acceptable?), D4, D5, D7, D8, D9, D10. All have defaults.

# REV 4 (user decisions 2026-11-17)

- **Approved for implementation:** Wave 0, R1, C1, C2, C5-mocked.
- **Accepted defaults:** D4, D5, D7, D8, D9.
- **D2 accepted:** the customer Odoo configuration prerequisite, with its documented residual risk.
- **D10 approved:** `ring` as a direct dependency. Before C3b merges it must pass two checks: a known-vector ES256 test and the JWT format check against the NetSuite doc fields.
- **NetSuite approval behaviour (L4)** stays a mandatory production validation gate.
- **No production-integration claim** until the live gates pass.

## Pre-C4b corrections (C4b plan must include these; the parent verifies them before approving C4b)

### P-1 Manual retry versus at-most-once

- New append-only table `workflow_execution_attempts`. Columns:
  - `execution_id`, `attempt_no`
  - `granted_by` (NULL for attempt 1), `grant_reason`
  - `write_intent_at`, `sent_at`, `outcome`, `evidence`
  - UNIQUE `(execution_id, attempt_no)`
- **Write intent = inserting the attempt row**, committed before HTTP. The insert is guarded:
  - `attempt_no = previous + 1`
  - `attempt_no <= 1 + count(grants)`
  - every earlier attempt has a resolved outcome
- **One POST per attempt row.** A new row exists only after a human grant (the reconcile permission, non-requester, reason required).
- **The externalId is the same across attempts**, so a late-landing earlier attempt is still found by reconciliation.

### P-2 Recovering execution ownership after lease expiry

- **Owner token = the claiming job's `jobs.locked_by`.**
- **Takeover rule:**
  ```sql
  UPDATE ... SET owner_token = $new
  WHERE id = $1
    AND status IN ('queued', 'running')
    AND NOT EXISTS (
      SELECT 1 FROM jobs
      WHERE locked_by = owner_token
        AND status = 'running'
        AND locked_until > now()
    )
  ```
- **After takeover:** if the latest attempt has `write_intent_at` set and no outcome, go to reconciliation (rule W1) and never POST.
- **The stale worker** cannot update state, because its fenced writes fail. It may only INSERT an append-only `workflow_events` row tagged with `attempt_no`, recording the HTTP status, returned id, and Location. Reconciliation uses that row as presence evidence and re-reads it.
- **Tests:**
  - takeover while the old lease is live is refused;
  - takeover after expiry is accepted;
  - a stale worker's late success is recorded as evidence and leads to `succeeded` via read-back, with 1 POST total.

### P-3 Financial precision

- **R1 adds O6.** Verify from the odoo 19.0 source how `purchase.order.line` computes `price_subtotal` and `amount_untaxed`:
  - the tax engine path;
  - the effect of `company.tax_calculation_rounding_method` (round_per_line vs round_globally);
  - currency rounding;
  - the price precision (decimal.precision "Product Price").
- **Then rewrite P1/P2 and T30 to match the verified rule**, instead of the assumed `round_half_away(qty × price)`.
- **Until O6 is verified,** C4a/C4b may not implement P1/P2.
