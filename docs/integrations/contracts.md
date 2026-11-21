# WF-01 frozen contracts (Wave 0)

Status: **frozen** for children C1, C2, C3a, C3b, C4a, C4b and C5. Source: `docs/integrations/PLAN.md` rev 2 with the REV 3 and REV 4 amendments. Where they conflict, the later amendment wins, and this file follows it.

- **Change rule:** a child that needs a different name, column, enum value, route or JSON field asks the parent. The child does not change it locally. The parent edits this file, bumps the version line below and tells every affected child.
- **Version:** `contracts v1.2 (2026-11-21)`: v1.1 (W0 review corrections) plus §11 (WF-02..WF-07 frozen names).
- **Facts rule:** every repo convention below cites `path:line`, read on base `45ca23d9f`. Paths are relative to `apps/api/` unless they start with `apps/`, `packages/` or `docs/`. Anything not checked is marked **OPEN** and listed in §9.
- **Provider wording:** Odoo and NetSuite field, model and error names appear only inside `src/integrations/providers/*` and are "per R1" (`docs/integrations/facts/*.md`). Shared code, tables and JSON use the neutral names below.

---

## 1. Repo conventions this contract relies on (verified)

| # | Convention | Evidence |
|---|---|---|
| K1 | Ids are `uuid PRIMARY KEY DEFAULT uuidv7()` (PG18 built-in). | `migrations/20261020060110_init_auth.up.sql:2,6` |
| K2 | Each tenant table has `workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE` and `CONSTRAINT <table>_workspace_id_id_key UNIQUE (workspace_id, id)`. Child tables point at it with a composite FK `FOREIGN KEY (workspace_id, x_id) REFERENCES x (workspace_id, id)`, so the database refuses a cross-workspace reference. | `docs/api/conventions.md` "Tables"; `migrations/20261115100000_accounting_core.up.sql:48,68,101-103` |
| K3 | Member references use `FOREIGN KEY (workspace_id, user_id) REFERENCES workspace_members (workspace_id, user_id)`. Audit rows that must outlive a membership use `REFERENCES users (id)`. | `migrations/20261020214853_warehouses.up.sql:46-51`; `migrations/20261109130000_farm_supply_corrections.up.sql:58` |
| K4 | Workspaces are soft-deleted (`deleted_at`), so a blanket "no UPDATE/DELETE" trigger does not block tenant cleanup. | `migrations/20261020060110_init_auth.up.sql:29` |
| K5 | Append-only tables use `BEFORE UPDATE OR DELETE … FOR EACH ROW EXECUTE FUNCTION <fn>()`, where the fn raises `ERRCODE = 'restrict_violation'`. | `migrations/20261101120000_farm_supply.up.sql:62-70`; `migrations/20261109130000_farm_supply_corrections.up.sql:81-88` |
| K6 | Four-eyes enforced in the schema: `CHECK (decided_by <> requested_by)` plus a composite FK to the request's `(id, requested_by)`. | `migrations/20261109130000_farm_supply_corrections.up.sql:52-62` |
| K7 | Enums are `text NOT NULL CHECK (col IN (...))`. `updated_at` is set by the repo SQL (`updated_at = now()`); there is no trigger. | `migrations/20261115100000_accounting_core.up.sql:40-47`; 63 files under `src/` contain `updated_at = now()` |
| K8 | Reversible migration pairs `YYYYMMDDHHMMSS_<name>.{up,down}.sql`, created with `sqlx migrate add -r`. The latest on base is `20261115140000_inventory_valuation`. | `.claude/skills/verzo-360-api/SKILL.md:86`; `ls migrations` |
| K9 | Routes are `/v1/workspace/{workspace}/<resource>[/{id}]`. The `#[utoipa::path]` path omits the `/v1` prefix, which is added by `.nest("/v1", v1)`. Update is `POST /{id}`, with no PUT or PATCH. | `docs/api/conventions.md` "Routes and errors"; `src/lib.rs:164`; `src/modules/farm_supply_corrections/handlers.rs:27` |
| K10 | Response shapes: create `201 {"data":{"<x>_id":"<uuid>"}}`; get `{"data":{…}}`; list `{"data":[…],"meta":{current_page,from,last_page,per_page,to,total}}`; update and delete `{"message":"ok"}`. Paging `page`/`page_size` defaults to 50 with a max of 100. | `docs/api/conventions.md` "Routes and errors", "Lists"; `src/http/list.rs:43-60`; `src/http/response.rs:11-30` |
| K11 | Guard order: 401, then 404 (outsider, or a malformed or unknown workspace), then 403 (inactive member or missing permission via `Can<P>`), then 422. An unknown or foreign `{id}` is 404 with `The requested resource could not be found.` | `src/http/workspace.rs:1-3,58-93,115-125`; `src/error.rs:85-87,278-282` |
| K12 | Permission markers: `permissions! { X => "group.action" }` plus `Can<X>`. A rule that depends on the row uses `access.allows(name)`. | `src/http/workspace.rs:52-55,96-139`; `src/modules/accounting/handlers.rs:32-38` |
| K13 | The permission catalog `GROUPS` is sorted (groups, then actions). `tests/api/permissions.rs:74` pins `api_enforced.len() == 258`. | `src/modules/permissions/catalog.rs:9,250-264,275-292`; `tests/api/permissions.rs:73-77` |
| K14 | Error bodies: `{"message"}`. A stable machine code uses `AppError::Reason { status, code: &'static str, message }`, which renders `{"message","code"}`. 422 field errors render `{"message","errors":{field:[…]}}`. | `src/error.rs:78-84,215-230,273-277`; `src/modules/accounting/service.rs:26-58` |
| K15 | `Idempotency-Key` (1-255 visible chars) uses extractor `idempotency::Keyed<T>`. The answer is stored in the write's transaction and replayed byte for byte. Same key with another body → 422 `Idempotency-Key reused with different request payload`. No header means no dedupe. The SPA helper is `useIdempotencyKey()`. | `src/idempotency.rs:1-7,25-27,82-131,146,212-250`; `packages/api-client/src/idempotency.ts:1-21` |
| K16 | Jobs: kind const + `KINDS` + `dispatch` arm; recurring kinds go in `SCHEDULES: [Schedule; N]` (global, fixed interval, dedupe `schedule:<name>`). `NewJob{concurrency_key, dedupe_key, max_attempts}`. `Claimed.token` = `jobs.locked_by`, a fresh token per claim. | `src/jobs/kinds.rs:1-2,21-63,65,87-140,170-213`; `src/jobs/scheduler.rs:58-65`; `src/jobs/repo.rs:20-75`; `migrations/20261023073000_jobs.up.sql:1-4,20-23` |
| K17 | `jobs.concurrency_key` is unique among running jobs across **all** workspaces. `dedupe_key` is unique per workspace among queued and running jobs. `jobs` has no index on `locked_by`; `jobs_running_idx` is `(workspace_id) WHERE status='running'`. | `migrations/20261023073000_jobs.up.sql:57-64` |
| K18 | Credentials: `CredentialKey::seal(workspace, path, plain)` / `open(…)`, XChaCha20-Poly1305 with AAD = workspace + path. `workspace_credentials` has PK `(workspace_id, path)`, a fixed path CHECK list, and `PATHS` mirrored and pinned by a test that counts the paths. GET prints the mask `********`. | `src/crypto.rs:19,66-114`; `migrations/20261021151609_credentials.up.sql:5-22`; `src/modules/settings/credentials.rs:5-56` |
| K19 | Outbound HTTP goes through `Outbound::send` (DNS guard against private and metadata addresses). | `src/outbound.rs:28,43` |
| K20 | Provider precedent: an enum with one variant per adapter and `async fn` methods; no `async_trait` crate and no trait objects. Errors are a small `thiserror` enum. | `src/payments/mod.rs:113-165`; `Cargo.toml` (no `async-trait`); edition 2024 (`Cargo.toml:4`) |
| K21 | Layers per module: `mod.rs, routes.rs, handlers.rs, service.rs, repo.rs, dto.rs, model.rs`, created only as needed. API docs go in `apps/api/docs/api/<feature>.md` (the folder exists: 67 files incl. `conventions.md`, `settings.md`, `jobs.md`), with entries in `apps/api/CHANGELOG.md`. There is no repo-root `docs/api/`. | `.claude/skills/verzo-360-api/SKILL.md:28-56,91-96`; `ls apps/api/docs/api` |
| K22 | Routes are mounted in `openapi::private()` with `.merge(<module>::routes::private())`. Each module's `routes.rs` lists `routes!(handlers::x)`. | `src/openapi.rs:140-251`; `src/modules/farm_supply_corrections/routes.rs:8-15` |
| K23 | Generated client: `apps/api/openapi.json` comes from `pnpm --filter @verzo/api openapi`, then `orval` writes `packages/api-client/src/generated/*`. | `apps/api/package.json:10`; `packages/api-client/orval.config.ts:7,10`; `packages/api-client/package.json:11-12` |
| K24 | Shared UI exports are `@verzo/ui/components/*` → `src/components/*.tsx`. `Notice{variant: info\|success\|warning\|error, title, children?, action?, live?}` and `EmptyState{title, description?, action?, icon?}`. | `packages/ui/package.json:2,41-47`; `packages/ui/src/components/notice.tsx:10,35-48`; `packages/ui/src/components/empty-state.tsx:6-19` |
| K25 | Quantities and rates are `numeric(24,8)`, stored as `Decimal` and rendered as JSON strings. Stored totals are `bigint` minor units (`i64`). | `docs/api/conventions.md` "Money and decimals" |

---

## 2. Tables

General rules for every table below: K1 id, K2 `workspace_id` + `<t>_workspace_id_id_key`, `created_at timestamptz NOT NULL DEFAULT now()`, plus `updated_at timestamptz NOT NULL DEFAULT now()` on mutable tables (K7). Text columns carry a `char_length` CHECK, shown as `text(a..b)`. "AO" = append-only trigger per K5.

### 2.1 C1 migration `<ts>_integrations.{up,down}.sql`

**`integration_connections`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | K1, K2 |
| provider | text | no | `CHECK (provider IN ('odoo','netsuite'))` |
| label | text(1..100) | no | |
| config | jsonb | no | `jsonb_typeof = 'object'`, ≤ 16 KiB; non-secret; validated per provider in Rust (§4.5) |
| status | text | no | `CHECK IN ('draft','active','degraded','unauthorized','expired','disconnected')`, default `'draft'` |
| status_reason | text(1..200) | yes | stable code (§3.6) |
| credential_expires_at | timestamptz | yes | user-entered |
| last_probe | jsonb | yes | `[CapabilityCheck]` (§4.3), array |
| last_probe_at | timestamptz | yes | |
| created_by | uuid | no | `REFERENCES users (id)` (K3 audit) |
| created_at, updated_at | timestamptz | no | |

Indexes: `UNIQUE (workspace_id, provider, lower(label))` named `integration_connections_label_key`.

**`integration_secrets`** (resolves D6; see §9 O-1)
| column | type | null | rule |
|---|---|---|---|
| workspace_id | uuid | no | |
| connection_id | uuid | no | FK `(workspace_id, connection_id) → integration_connections (workspace_id, id) ON DELETE CASCADE` |
| name | text | no | `CHECK (name IN ('api_key','private_key'))` |
| key_id | text | no | `CHECK (key_id ~ '^[0-9a-f]{16}$')` (copied from K18) |
| sealed | bytea | no | `CHECK (octet_length(sealed) BETWEEN 40 AND 16040)` (values ≤ 4000 chars, PEM) |
| updated_by | uuid | yes | `REFERENCES users (id) ON DELETE SET NULL` |
| created_at, updated_at | timestamptz | no | |

PK `(connection_id, name)`. Seal path = `integration:<connection_id>:<name>`, passed to `CredentialKey::seal(workspace_id, path, plain)`.

**`integration_pairs`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| workflow_key | text | no | `CHECK (workflow_key IN ('wf01_purchase'))` |
| source_connection_id, target_connection_id | uuid | no | composite FKs to `integration_connections` |
| status | text | no | `CHECK IN ('draft','active','paused')`, default `'draft'` |
| paused_reason | text(1..200) | yes | `CHECK ((status = 'paused') = (paused_reason IS NOT NULL))` |
| config | jsonb | no | object: `{poll_minutes:int 1..1440 (default 5), backfill_days:int 0..365 (default 0), full_scan_minutes:int 15..1440 (default 60), succeeded_watch_days:int 1..365 (default 30), tax_tolerance_minor:int ≥0 (default 0), default_locations:{<subsidiary target_id>: <location target_id>}}`, validated in Rust |
| activated_at | timestamptz | yes | backfill anchor |
| last_polled_at, last_swept_at, last_full_scan_at | timestamptz | yes | scheduler stamps (§6) |
| created_by | uuid | no | `REFERENCES users (id)` |
| created_at, updated_at | timestamptz | no | |

Indexes: `UNIQUE (workspace_id, workflow_key, source_connection_id, target_connection_id)`; `CHECK (source_connection_id <> target_connection_id)`.

**`integration_mappings`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| pair_id | uuid | no | FK `(workspace_id, pair_id) → integration_pairs ON DELETE CASCADE` |
| entity | text | no | `CHECK IN ('company','vendor','product','uom','currency','user','location')` |
| source_id | text(1..100) | no | external id as text |
| target_id | text(1..100) | no | for `user`: a member's `users.id` uuid as text; the service checks it is an active member |
| source_label, target_label | text(0..200) | yes | display only |
| created_by, updated_by | uuid | no / yes | `REFERENCES users (id)` |
| created_at, updated_at | timestamptz | no | |

Index: `UNIQUE (pair_id, entity, source_id)`.

**`integration_poll_cursors`**: `workspace_id`, `connection_id` (composite FK, CASCADE), `stream text(1..100)`, `cursor_ts timestamptz NOT NULL`, `cursor_id bigint NOT NULL DEFAULT 0`, `updated_at`. PK `(connection_id, stream)`. WF-01 stream name: `wf01_purchase:<pair_id>`.

**`external_links`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| correlation_id | text | no | `CHECK (correlation_id ~ '^vz-[0-9a-f]{32}$')` |
| source_connection_id | uuid | no | composite FK |
| source_model | text(1..100) | no | |
| source_id | text(1..100) | no | |
| target_connection_id | uuid | no | composite FK |
| target_type | text(1..100) | no | |
| target_id | text(1..100) | yes | `CHECK ((state = 'linked') <= (target_id IS NOT NULL))` |
| state | text | no | `CHECK IN ('pending','linked','conflict')` |
| created_at, updated_at | timestamptz | no | |

Index: `UNIQUE (workspace_id, correlation_id)`.

### 2.2 C2 migration `<ts>_approvals_workflows.{up,down}.sql`

**`approval_policies`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| workflow_key | text | no | `CHECK IN ('wf01_purchase')` |
| currency_ref | text(1..64) | no | (O-3) |
| threshold_minor | bigint | no | `CHECK (threshold_minor >= 0)` |
| active | boolean | no | default true |
| created_by | uuid | no | `REFERENCES users (id)` |
| created_at, updated_at | timestamptz | no | |

Index: `UNIQUE (workspace_id, workflow_key, currency_ref) WHERE active`. There is no `approver_permission` column; see the rule in §3.1 (O-2).

**`approval_requests`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| workflow_key | text | no | `CHECK IN ('wf01_purchase')` |
| pair_id | uuid | no | composite FK → `integration_pairs` |
| subject_ref | text(1..300) | no | `odoo:<connection_id>:purchase.order:<id>` for WF-01 |
| revision | integer | no | `CHECK (revision >= 1)` |
| fingerprint | text | no | `CHECK (fingerprint ~ '^[0-9a-f]{64}$')` |
| snapshot | jsonb | no | §5.4, ≤ 256 KiB. Holds every mapped **target id**, the `mapping_ids`, and the target connection, so C4b's `mapping_changed` check compares snapshot vs current mappings without other data |
| amount_minor | bigint | no | untaxed, `CHECK (amount_minor >= 0)` |
| currency_ref | text(1..64) | no | |
| currency_decimals | smallint | no | `CHECK (currency_decimals BETWEEN 0 AND 6)` |
| required_permission | text | yes | `CHECK IN ('integration.approve','integration.approve_high')`; NULL only while `blocked` |
| requester_user_id | uuid | yes | `REFERENCES users (id)`; NULL → blocked `no_requester` |
| status | text | no | `CHECK IN ('blocked','pending','approved','rejected','returned','invalidated')` |
| blocked_reasons | text[] | no | default `'{}'`; `CHECK ((status = 'blocked') = (cardinality(blocked_reasons) > 0))`; codes per §3.1 |
| decided_by | uuid | yes | `REFERENCES users (id)`; `CHECK (decided_by IS NULL OR decided_by <> requester_user_id)` |
| decided_at | timestamptz | yes | `CHECK ((status IN ('approved','rejected','returned')) = (decided_by IS NOT NULL AND decided_at IS NOT NULL))` |
| invalidated_reason | text | yes | `CHECK ((status = 'invalidated') = (invalidated_reason IS NOT NULL))`; codes §3.1 |
| resubmitted_by | uuid | yes | `REFERENCES users (id)`; set when a same-fingerprint revision was created by an explicit resubmit |
| created_at, updated_at | timestamptz | no | |

Indexes:
- `UNIQUE (workspace_id, workflow_key, subject_ref) WHERE status IN ('blocked','pending','approved')` (`approval_requests_one_live`). R3-§4 keeps it; note that `approved` stays live forever, so a later revision of the same subject while approved is handled per R3-§5, never by a second live row.
- `UNIQUE (workspace_id, workflow_key, subject_ref, revision)`
- `UNIQUE (id, requester_user_id)` (four-eyes FK target, K6)
- list: `(workspace_id, status, id DESC)`

**`approval_decisions`** (AO)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| request_id | uuid | no | composite FK → `approval_requests` |
| requester_user_id | uuid | no | FK `(request_id, requester_user_id) → approval_requests (id, requester_user_id)` (K6) |
| actor_user_id | uuid | no | `REFERENCES users (id)`; `CHECK (actor_user_id <> requester_user_id)` |
| decision | text | no | `CHECK IN ('approve','reject','return')` |
| comment | text(0..2000) | yes | `CHECK (decision = 'approve' OR comment IS NOT NULL)` |
| fingerprint_seen | text | no | 64 hex |
| permission_used | text | no | `integration.approve` or `integration.approve_high` |
| created_at | timestamptz | no | `DEFAULT clock_timestamp()` |

Index: `UNIQUE (request_id)`. A request gets at most one decision; the CAS in §3.1 decides the race and this index is the backstop.

**`workflow_executions`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| approval_request_id | uuid | no | composite FK; `UNIQUE (approval_request_id)` |
| workflow_key | text | no | `CHECK IN ('wf01_purchase')` |
| correlation_id | text | no | `^vz-[0-9a-f]{32}$`; `UNIQUE (workspace_id, correlation_id)` |
| status | text | no | `CHECK IN ('queued','running','blocked','awaiting_reconciliation','cancelled','succeeded','succeeded_with_variance','resolved_manual','failed')` |
| phase | text | no | `CHECK IN ('prechecking','write_intent','write_sent','verifying','done')`, default `'prechecking'` |
| reason | text(1..100) | yes | stable code (§3.2); required when status is `blocked`, `cancelled`, `failed` or `resolved_manual` |
| resolution | text | yes | `CHECK IN ('linked_with_differences','closed_no_po','variance_accepted')`; `CHECK ((status = 'resolved_manual') = (resolution IS NOT NULL))` |
| owner_token | uuid | yes | = the claiming job's `jobs.locked_by` (`Claimed.token`, K16). Set by claim or takeover only (SQL in §3.2); every later state write is fenced on it |
| remote_attempts | smallint | no | default 0; `CHECK (remote_attempts >= 0)` |
| manual_retry_grants | smallint | no | default 0; `CHECK (manual_retry_grants >= 0)` |
| error_class | text | yes | `CHECK IN ('transient','ambiguous','auth','permission','not_found','validation','conflict','unknown')` |
| error_detail | text(0..1000) | yes | readable, never a secret |
| target_ref | text(1..300) | yes | `netsuite:<connection_id>:purchaseOrder:<id>` for WF-01 |
| created_at, updated_at | timestamptz | no | |

CHECKs:
- `remote_attempts <= 1 + manual_retry_grants`
- `status <> 'cancelled' OR remote_attempts = 0` (I2)
- `status NOT IN ('succeeded','succeeded_with_variance') OR target_ref IS NOT NULL`
- `resolution IS DISTINCT FROM 'linked_with_differences' OR target_ref IS NOT NULL`

Indexes: `(workspace_id, status, id DESC)`; `(status) WHERE status IN ('queued','awaiting_reconciliation')` for sweeps.

I3 (`succeeded` ⇒ a `verify_match` event exists) is enforced in the C2 helper `finish_succeeded`, which inserts the event and updates status in one transaction. It is not a constraint.

**`workflow_execution_attempts`** (P-1; AO except set-once columns)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| execution_id | uuid | no | composite FK |
| attempt_no | smallint | no | `CHECK (attempt_no >= 1)`; `UNIQUE (execution_id, attempt_no)` |
| granted_by | uuid | yes | `REFERENCES users (id)`; `CHECK ((attempt_no = 1) = (granted_by IS NULL))` |
| grant_reason | text(1..1000) | yes | `CHECK ((granted_by IS NULL) = (grant_reason IS NULL))` |
| write_intent_at | timestamptz | no | default `clock_timestamp()`; the row insert **is** the write intent |
| sent_at | timestamptz | yes | set once |
| outcome | text | yes | `CHECK IN ('created','rejected','conflict','ambiguous','not_sent')`; set once |
| evidence | jsonb | yes | set once; `{http_status?, target_id?, location?, error_code?}` |

Trigger `workflow_execution_attempts_set_once`: UPDATE may only change `sent_at`, `outcome` or `evidence`, and only from NULL to a value; DELETE raises `restrict_violation` (K5 style).

Insert guard (C2 helper `begin_attempt`, one transaction with the CAS in §3.2):
- `attempt_no = remote_attempts + 1` of the locked execution row;
- `attempt_no <= 1 + manual_retry_grants`;
- every earlier attempt has `outcome IS NOT NULL`.

The same transaction sets `workflow_executions.remote_attempts = attempt_no` and `phase = 'write_intent'`.

**`workflow_events`** (AO)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| execution_id | uuid | yes | composite FK |
| approval_request_id | uuid | yes | composite FK; `CHECK (execution_id IS NOT NULL OR approval_request_id IS NOT NULL)` |
| attempt_no | smallint | yes | P-2 stale-worker evidence: set on `api_result` / `stale_worker_result` rows. Inserting an event needs **no** `owner_token` fence; only state writes on `workflow_executions` are fenced. |
| kind | text | no | `CHECK (kind ~ '^[a-z][a-z0-9_]{0,63}$')`; no list, like `jobs.kind` (K16); WF-01 kinds frozen in §3.7 |
| payload | jsonb | no | object, `octet_length(payload::text) <= 65536`; never secrets, headers or raw bodies |
| actor_kind | text | no | `CHECK IN ('user','system')` |
| actor_user_id | uuid | yes | `REFERENCES users (id)`; `CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))` |
| created_at | timestamptz | no | `clock_timestamp()` |

Index: `(execution_id, id)`, `(approval_request_id, id)`.

**`workflow_incidents`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| kind | text | no | `CHECK IN ('duplicate_obligation_risk','target_conflict','source_gone_after_execution','connection_unhealthy','odoo_control_drift','source_changed_during_write','source_changed_after_execution','tax_variance')` |
| subject_ref | text(1..300) | no | |
| pair_id, connection_id, approval_request_id, execution_id | uuid | yes | composite FKs |
| status | text | no | `CHECK IN ('open','resolved')` |
| detail | jsonb | no | object ≤ 16 KiB, readable facts only |
| resolved_by | uuid | yes | `REFERENCES users (id)` |
| resolved_at | timestamptz | yes | `CHECK ((status = 'resolved') = (resolved_by IS NOT NULL AND resolved_at IS NOT NULL))` |
| note | text(1..2000) | yes | required on resolve |
| created_at, updated_at | timestamptz | no | |

Index: `UNIQUE (workspace_id, kind, subject_ref, coalesce(execution_id, '00000000-0000-0000-0000-000000000000'::uuid)) WHERE status = 'open'` (`workflow_incidents_one_open`). A sweep that finds the same problem again does not open a second incident.

**`workflow_resolutions`** (AO)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| execution_id | uuid | yes | composite FK |
| incident_id | uuid | yes | composite FK; `CHECK (execution_id IS NOT NULL OR incident_id IS NOT NULL)` |
| action | text | no | `CHECK IN ('link_existing','allow_retry','close_no_po','accept_variance','reject_variance','retry_after_fix','resolve_incident')` |
| actor_user_id | uuid | no | `REFERENCES users (id)` |
| reason | text(1..2000) | no | |
| target_id | text(1..100) | yes | for `link_existing` |
| outcome | text(1..100) | no | resulting status/resolution, e.g. `succeeded`, `resolved_manual:linked_with_differences` |
| created_at | timestamptz | no | `clock_timestamp()` |

### 2.3 Migration order

C1 migration timestamp < C2 migration timestamp, because C2 references `integration_pairs`. Both must be later than `20261115140000` and than any M3 migration on main at merge time. The parent renumbers if needed (PLAN §10).

---

## 3. State machines

The column on the left of each table is the only writer allowed to make that move. A move not listed is refused (service error, never a silent no-op).

### 3.1 Approval request (`approval_requests.status`)

| from | to | trigger | writer |
|---|---|---|---|
| (new) | `blocked` | evaluate finds ≥1 reason | C4a `evaluate` via C2 `requests::upsert_revision` |
| (new) | `pending` | evaluate clean | same |
| `blocked` | `pending` | re-evaluate clean (sweep) | C4a via C2 `requests::unblock` |
| `blocked` | `blocked` | reasons changed | same (updates `blocked_reasons`) |
| `pending` | `approved` | approve decision | C2 decide (CAS) |
| `pending` | `rejected` | reject decision (comment required) | C2 decide |
| `pending` | `returned` | return decision (comment required) | C2 decide |
| `blocked`, `pending` | `invalidated` | source changed, cancelled, gone or ineligible | C4a sweep/poll via C2 `requests::invalidate` |
| `approved` (no execution row) | `invalidated` | same | same; takes `FOR UPDATE` on the request and refuses if an execution row exists |
| `approved` (execution `queued`/`prechecking`) | `invalidated` | executor pre-check sees source change | C4b only, same transaction as execution → `cancelled` |

Terminal: `rejected`, `returned`, `invalidated`.

Decide CAS (one transaction):
```sql
UPDATE approval_requests SET status = $to, decided_by = $actor, decided_at = now(), updated_at = now()
WHERE workspace_id = $ws AND id = $id AND status = 'pending' AND fingerprint = $fingerprint_seen
```
- If 0 rows change, re-read the row. Not pending → 409 `approval_already_decided`. Fingerprint differs → 409 `approval_stale`.
- Then INSERT `approval_decisions`. On `approve`, in the same transaction, INSERT `workflow_executions` (status `queued`) and call `workflows::dispatch::enqueue_execution` (§4.6).

Decide checks, in this order, after the K11 guards:
1. The actor is not `requester_user_id` → else 403 `four_eyes_required`. The owner gets no bypass.
2. The actor holds `required_permission`. For `integration.approve`, holding `integration.approve_high` also passes. Else 403 `This action is unauthorized.`
3. Then the CAS.

Required permission (D4):
- `amount_minor < threshold_minor` → `integration.approve`;
- otherwise → `integration.approve_high`.
- The comparison is done in SQL, against the active policy for `(workflow_key, currency_ref)`.

`blocked_reasons` codes, format `code[:entity[:source_id]]`:
- `unmapped:<company|vendor|product|uom|currency|user|location>:<source_id>`
- `no_policy:<currency_ref>`
- `no_requester`
- `no_eligible_approver`
- `amount_not_exact`
- `rate_precision`
- `untotalled_lines` (P2)
- `downpayment_line` (C6-O1: the request has an Odoo down-payment line, which WF-01 does not support; once per request)
- P1/P2 computation is **per R1 O6** (PLAN P-3). Only the codes are frozen; no rounding formula is.
- `connection_inactive:<connection_id>`

`invalidated_reason` codes: `source_changed`, `source_cancelled`, `source_gone`, `source_ineligible`.

Revisions (R3-§4): a new row gets `revision = max(revision for subject) + 1`.
- Poll with an unchanged fingerprint whose latest row is `rejected` or `returned` → no row; event `skipped_previously_decided`.
- Latest row `invalidated` → new revision allowed.
- `POST …/resubmit` creates a new revision with the same fingerprint and sets `resubmitted_by`.

### 3.2 Execution (`workflow_executions.status`)

| from | to | condition | writer |
|---|---|---|---|
| (new) | `queued` | approve | C2 decide |
| `queued` | `running` | claim CAS `WHERE status='queued'` sets `owner_token` | C4b via C2 `executions::claim` |
| `running` | `queued` | Transient **before** write intent; or 429 | C4b (fenced) |
| `running` | `blocked` | Auth, Permission, Validation, NotFound mapping, connection inactive, `mapping_changed`, `approver_revoked`, `rate_precision` (all before write intent; or a definite `rejected` outcome) | C4b (fenced) |
| `running` | `cancelled` | pre-check source change, `remote_attempts = 0` | C4b (fenced) + approval `invalidated` |
| `running` | `awaiting_reconciliation` | any re-entry with phase ≥ `write_intent` (W1); Ambiguous; Conflict; verify mismatch (V1/V2) | C4b (fenced) |
| `running` | `succeeded` | verify match (V1, V2, V3 within tolerance) | C4b via `finish_succeeded` |
| `running` | `succeeded_with_variance` | V1+V2 match, tax difference > tolerance | C4b; also opens `tax_variance` |
| `awaiting_reconciliation` | `succeeded` / `succeeded_with_variance` | reconcile finds the PO and it matches | C4b reconcile |
| `awaiting_reconciliation` | `queued` | human `allow_retry` (grant +1, phase → `prechecking`) | C2 resolve route |
| `awaiting_reconciliation` | `succeeded` | human `link_existing`, read-back matches | C2 route → C4b fn |
| `awaiting_reconciliation` | `resolved_manual` | `link_existing` mismatch (`linked_with_differences`), or `close_no_po` | C2 route (→ C4b fn for link) |
| `succeeded_with_variance` | `resolved_manual` | `accept_variance` (`variance_accepted`) | C2 route |
| `blocked` | `queued` | human `retry_after_fix`; all pre-checks rerun | C2 route |
| `queued`, `blocked` | `failed` | job attempts exhausted | C4b |

Terminal: `cancelled`, `succeeded`, `resolved_manual`, `failed`. `succeeded_with_variance` is open until reviewed. A `reject_variance` keeps the status and leaves the incident open.

`reason` codes:
- blocked: `connection_unauthorized`, `connection_expired`, `connection_inactive`, `permission_denied`, `validation_rejected`, `mapping_stale`, `mapping_changed`, `approver_revoked`, `rate_precision`, `untotalled_lines`, `downpayment_line`, `pair_paused`
- cancelled: `source_changed`
- failed: `attempts_exhausted`
- resolved_manual: copy of `resolution`

Takeover (P-2), the only other way to set `owner_token`:
```sql
UPDATE workflow_executions SET owner_token = $new, updated_at = now()
WHERE workspace_id = $ws AND id = $id AND status IN ('queued','running')
  AND NOT EXISTS (SELECT 1 FROM jobs j WHERE j.workspace_id = $ws AND j.locked_by = workflow_executions.owner_token
                  AND j.status = 'running' AND j.locked_until > now())
```
`j.workspace_id = $ws` is there so `jobs_running_idx` (K17) serves the probe. Every later write ends with `AND owner_token = $tok` (fencing).

### 3.3 Execution phase

```
prechecking ──begin_attempt (CAS + attempt row)──► write_intent ──HTTP bytes sent──► write_sent ──2xx id──► verifying ──match/mismatch──► done
```
- `allow_retry` puts the phase back to `prechecking`.
- W1: while phase ∈ {`write_intent`,`write_sent`,`verifying`} and the latest attempt has `outcome IS NULL`, the executor never POSTs; it goes to reconciliation.

### 3.4 Attempt outcome (`workflow_execution_attempts.outcome`)

`NULL` (in flight or unknown) → exactly one of:
- `created` (2xx with an id)
- `rejected` (definite 4xx validation, auth or permission)
- `conflict` (eid conflict, N3)
- `ambiguous` (timeout, 5xx after send, reset, or owner lost; set by reconcile or takeover)
- `not_sent` (proved no byte left)

The value is set once. `allow_retry` requires the latest outcome ∈ {`ambiguous`,`not_sent`,`rejected`}.

### 3.5 Incident

`open → resolved` (needs `integration.reconcile`, the actor is not the requester of the linked request, and `note` is required). There is no reopen; a recurrence opens a new row once the old one is resolved.

### 3.6 Connection status

| from | to | trigger | writer |
|---|---|---|---|
| (new) | `draft` | create | C1 |
| `draft`, `degraded`, `unauthorized`, `expired` | `active` | probe all-ok and expiry > 14 days away | C1 test route |
| `active` | `degraded` | probe partial / Permission error / expiry ≤ 14 days | C1 probe, C3 adapters via C1 `connections::mark` |
| any non-`disconnected` | `unauthorized` | Auth error after one refresh | same |
| any non-`disconnected` | `expired` | `credential_expires_at` < now (checked on probe and on every job start) | same |
| any | `disconnected` | `POST …/disconnect` | C1 |
| `disconnected` | `draft` | `POST …/{connection}` with new secret | C1 |

`status_reason` codes: `probe_failed:<capability_key>`, `credential_expiring`, `credential_expired`, `auth_failed`, `permission_denied`.

### 3.7 Pair status and WF-01 event kinds

Pair: `draft → active` (both connections `active` + WF-01 activation check); `active → paused` (control drift, connection not active, or manual); `paused → active` (same checks as activation).
- `paused_reason` codes: `odoo_control_drift`, `connection_inactive`, `manual`.

`workflow_events.kind` for WF-01:
- `trigger`, `snapshot`, `mapping_result`, `proposal`, `skipped_previously_decided`, `resubmitted`, `decision`, `invalidated`
- `claim`, `takeover`, `precheck_failed`, `write_intent`, `api_request`, `api_result`
- `verify_match`, `verify_mismatch`, `reconcile_check`, `stale_worker_result`, `incident_opened`, `resolution`

---

## 4. Rust contract

### 4.1 Module paths

```
src/integrations/mod.rs              C1  //! provider-agnostic glue
src/integrations/capability.rs       C1  CapabilityKey, CapabilityCheck, ErrorClass, DTOs, traits
src/integrations/connections/        C1  {mod,routes,handlers,service,repo,dto}.rs + secrets.rs
src/integrations/pairs/              C1  same layout
src/integrations/mappings/           C1  same layout
src/integrations/links.rs            C1
src/integrations/cursors.rs          C1
src/integrations/probe.rs            C1  probe runner + FakeProbe (cfg(test) / debug)
src/integrations/providers/mod.rs    C1  enums SourceClient / SinkClient (K20 style), arms added by C3
src/integrations/providers/odoo/     C3a {mod,client,source,probe,errors}.rs
src/integrations/providers/netsuite/ C3b {mod,auth,client,sink,probe,errors}.rs
src/modules/approvals/               C2  {mod,routes,handlers,service,repo,dto,model}.rs (requests, decisions, policies)
src/workflows/mod.rs                 C2
src/workflows/executions.rs          C2  claim, takeover, transition, begin_attempt, finish_succeeded
src/workflows/events.rs              C2  record_event
src/workflows/incidents.rs           C2  open (idempotent), resolve
src/workflows/dispatch.rs            C2  enqueue_execution(workflow_key) — arm body filled by C4b
src/workflows/routes.rs, handlers.rs, dto.rs   C2
src/workflows/wf01_purchase/         C4a {mod,poll,sweep,full_scan,fingerprint,evaluate,activation}.rs
                                     C4b {execute,reconcile,verify,link}.rs
```
`lib.rs` gets `mod integrations;` (C1) and `mod workflows;` (C2), private like `mod modules;` (`src/lib.rs:19`).

### 4.2 Shared types (`integrations/capability.rs`, C1)

```rust
/// Failure class of any provider call (PLAN §3, §6d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass { Transient, Ambiguous, Auth, Permission, NotFound, Validation, Conflict, Unknown }

/// Provider error: class + readable, secret-free detail + provider code (per R1).
#[derive(Debug, Clone, thiserror::Error)]
#[error("{class:?}: {detail}")]
pub struct ProviderError { pub class: ErrorClass, pub detail: String, pub code: Option<String>, pub http_status: Option<u16> }

/// Static capability keys. No dynamic registry.
pub type CapabilityKey = &'static str;
pub const PURCHASE_REQUEST_LIST: CapabilityKey = "purchase_request.list";
pub const PURCHASE_REQUEST_READ: CapabilityKey = "purchase_request.read";
pub const SOURCE_CONTROLS_READ: CapabilityKey = "source_controls.read";   // R3-§2 / O5
pub const PURCHASE_ORDER_FIND: CapabilityKey = "purchase_order.find_by_external_id";
pub const PURCHASE_ORDER_CREATE: CapabilityKey = "purchase_order.create";
pub const PURCHASE_ORDER_READ: CapabilityKey = "purchase_order.read";
pub const AUTH_TOKEN: CapabilityKey = "auth.token";                       // NetSuite M2M
pub const ODOO_CAPABILITIES: &[CapabilityKey] = &[PURCHASE_REQUEST_LIST, PURCHASE_REQUEST_READ, SOURCE_CONTROLS_READ];
pub const NETSUITE_CAPABILITIES: &[CapabilityKey] = &[AUTH_TOKEN, PURCHASE_ORDER_FIND, PURCHASE_ORDER_CREATE, PURCHASE_ORDER_READ];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilityCheck { pub key: String, pub ok: bool, pub error_class: Option<ErrorClass>, pub detail: Option<String> }

/// Exact money: integer minor units of `currency_ref`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Money { pub minor: i64, pub currency_ref: String }

pub struct SourceRef { pub id: String, pub changed_at: chrono::DateTime<chrono::Utc> }
pub struct Cursor { pub ts: chrono::DateTime<chrono::Utc>, pub id: i64 }
pub struct Page<T> { pub items: Vec<T>, pub next: Option<Cursor> }

/// Neutral purchase request (WF-01 source). Decimals are exact (`rust_decimal`).
pub struct PurchaseRequest {
    pub id: String, pub state: SourceState, pub company_id: String, pub vendor_id: String,
    pub currency_id: String, pub currency_ref: String, pub currency_decimals: u8,
    pub buyer_id: Option<String>, pub location_id: Option<String>, pub date_order: Option<String>,
    pub lines: Vec<RequestLine>, pub amount_untaxed: Decimal, pub amount_tax: Decimal, pub amount_total: Decimal,
    pub labels: std::collections::BTreeMap<String, String>, // "vendor:<id>" → display name, when the source gives one
}
pub enum SourceState { Draft, Sent, ToApprove, Confirmed, Cancelled, Other(String) }
pub struct RequestLine {
    pub id: String, pub product_id: String, pub uom_id: String, pub quantity: Decimal, pub price_unit: Decimal,
    pub subtotal: Decimal, pub tax_ids: Vec<String>, pub date_planned: Option<String>,
    pub downpayment: bool, // C6-O1: Odoo `is_downpayment`; product_id/uom_id are "" when the line has none
}
pub struct SourceControls { pub company_id: String, pub two_step: bool, pub threshold_zero: bool } // R3-§2

/// What Flow sends (values from the snapshot, ids already mapped).
pub struct NewPo { pub external_id: String, pub vendor_id: String, pub subsidiary_id: String,
                   pub currency_id: String, pub location_id: Option<String>, pub lines: Vec<NewPoLine> }
pub struct NewPoLine { pub item_id: String, pub unit_id: Option<String>, pub quantity: Decimal, pub rate: Decimal }
/// What the target reports (read-back for verify).
pub struct TargetPo { pub id: String, pub external_id: Option<String>, pub vendor_id: String, pub subsidiary_id: String,
                      pub currency_id: String, pub lines: Vec<NewPoLine>, pub untaxed: Option<Decimal>, pub tax: Option<Decimal> }
pub enum CreateOutcome { Created { id: String, location: Option<String> }, Ambiguous(ProviderError), Rejected(ProviderError), Conflict(ProviderError) }
```
Timestamps are `chrono` (`Cargo.toml:13`). Secrets in memory are `modules::auth::token::PlainSecret` (`src/modules/auth/token.rs:11`).

### 4.3 Traits

The repo has no trait-object or `async_trait` precedent (K20). The traits are therefore static-dispatch only; the call site holds the provider enum.

```rust
pub trait PurchaseRequestSource {
    async fn list_changed(&self, company_ids: &[String], cursor: &Cursor, limit: u32) -> Result<Page<SourceRef>, ProviderError>;
    async fn list_eligible_ids(&self, company_id: &str, after_id: i64, limit: u32) -> Result<Vec<String>, ProviderError>; // full scan
    /// Absent ids are returned in `.1` (gone).
    async fn read(&self, ids: &[String]) -> Result<(Vec<PurchaseRequest>, Vec<String>), ProviderError>;
    async fn read_controls(&self, company_ids: &[String]) -> Result<Vec<SourceControls>, ProviderError>;
}
pub trait PurchaseOrderSink {
    async fn find_by_external_id(&self, eid: &str) -> Result<Option<TargetPo>, ProviderError>;
    async fn create(&self, po: &NewPo) -> CreateOutcome;
    async fn read(&self, id: &str) -> Result<TargetPo, ProviderError>;
}
pub trait ConnectionProbe {
    async fn test(&self) -> Vec<CapabilityCheck>;
}
```
- `providers/mod.rs` (C1) declares `pub enum SourceClient { Odoo(odoo::Client) }` and `pub enum SinkClient { NetSuite(netsuite::Client) }`, which impl the traits by delegation (K20). C3a and C3b each add their variant and arm.
- Builders: `SourceClient::for_connection(ctx: &ConnCtx, row: &ConnectionRow) -> Result<Self, ProviderError>`. `ConnCtx` holds `Outbound`, `Arc<CredentialKey>` and `PgPool`.
- Fakes are in-process axum servers (PLAN §8). The real clients point at them, so there is no fake variant in production code. C1's `FakeProbe` is test-only.

### 4.4 C1 service API used by others

- `connections::load(db, ws, id) -> ConnectionRow`
- `connections::mark(db, ws, id, status, reason)`: the only status writer outside the C1 routes.
- `secrets::open(key, ws, connection_id, name) -> PlainSecret`. Never logged; `PlainSecret` per SKILL §5.
- `mappings::resolve(db, pair_id, entity, &[source_id]) -> BTreeMap<source_id, target_id>`
- `pairs::load_active(db) -> Vec<PairRow>`
- `pairs::set_status(db, ws, pair, status, reason)`
- `cursors::get/advance(tx, connection_id, stream, Cursor)`: advance only inside the page's transaction.
- `links::upsert(tx, ws, correlation_id, …, state)`

### 4.5 Connection config (non-secret, `config` jsonb)

- **odoo:** `{ "base_url": "https://…" (no path, https, passes Outbound), "database": string|null }`. Secret: `api_key`.
- **netsuite:** `{ "rest_domain": "<host>", "environment": "production"|"sandbox", "client_id": string, "certificate_id": string, "alg": "ES256" }`. Secret: `private_key` (PEM, EC P-256 per R3-D10).
- Any other key → 422.

### 4.6 C2 helpers used by C4

All of these take `&mut PgConnection` (inside the caller's transaction) unless noted:
- `requests::upsert_revision(tx, NewRevision) -> RequestRow`
- `requests::invalidate(tx, ws, id, reason)`
- `requests::unblock(tx, ws, id, required_permission)`
- `requests::set_blocked(tx, ws, id, reasons)`
- `executions::claim(db, ws, id, token) -> bool`
- `executions::takeover(db, ws, id, token) -> bool`
- `executions::transition(db, ws, id, token, from: &[Status], to, Patch) -> bool`: row-count checked and fenced. `Patch{reason, error_class, error_detail, target_ref, phase}`.
- `executions::begin_attempt(tx, ws, id, token, granted: Option<(Uuid, String)>) -> Option<i16>`: `None` means no POST.
- `executions::record_attempt_result(db, ws, id, attempt_no, sent_at, outcome, evidence)`: allowed for stale workers, set-once.
- `executions::finish_succeeded(tx, ws, id, token, target_ref, variance: bool)`: writes `verify_match` first (I3).
- `events::record(conn, ws, EventRef, kind, payload, actor)`
- `incidents::open(conn, ws, kind, subject_ref, refs, detail)`: idempotent through `workflow_incidents_one_open`.
- `dispatch::enqueue_execution(tx, workflow_key, execution_id)`: C2 ships the match with arm `"wf01_purchase" => Ok(())`; C4b replaces that arm body with the job enqueue (§6).

---

## 5. HTTP contract

All paths below are relative to `/v1` (K9). All `{id}` path params are uuids; a malformed, unknown or other-workspace id → 404 (K11). The `#[utoipa::path]` tags are `integrations`, `approvals` and `workflows`.

### 5.1 Permissions (C1 edits the catalog)

Add group `"integration"` with actions `["approve","approve_high","manage","reconcile","view"]` to `GROUPS` in sorted position (K13). Update the header comment counts and `tests/api/permissions.rs:74` from 258 to **263**.
- C1 marker types: `IntegrationView`, `IntegrationManage` (`src/integrations/*/handlers.rs`).
- C1 adds **all five** names (incl. `approve`, `approve_high`, `reconcile`) so C2 only uses them. Team grants are checked against the catalog (`catalog::assignable`, `src/modules/permissions/catalog.rs:250`), so C2 cannot grant unknown names. C1 makes the catalog change (catalog.rs + `tests/api/permissions.rs:74`) its **first standalone commit** and tells the parent; the parent cherry-picks that commit onto C2's branch. C2 never edits catalog.rs itself.
- C2 marker types: `IntegrationView`, `IntegrationReconcile`, `IntegrationManage` (`src/modules/approvals/handlers.rs`, `src/workflows/handlers.rs`). Approve and approve_high are row-dependent and use `access.allows` (K12).

### 5.2 Integrations (C1)

| method + path | permission | body | success | errors |
|---|---|---|---|---|
| GET `/workspace/{workspace}/integrations/connections` | view | — | list of `Connection` | |
| POST `/workspace/{workspace}/integrations/connections` | manage | `ConnectionInput` | 201 `{"data":{"connection_id"}}` | 422 fields, 422 `NO_KEY` text (K18), 422 label taken |
| GET `/workspace/{workspace}/integrations/connections/{connection}` | view | — | `{"data":Connection}` | 404 |
| POST `/workspace/{workspace}/integrations/connections/{connection}` | manage | `ConnectionInput` (patch; secret `"********"` keeps the stored one; a changed `base_url`/`rest_domain` with the mask → 422 `Enter this credential again.` as K18 `BOUND`) | `{"message":"ok"}` | 404, 422 |
| POST `/workspace/{workspace}/integrations/connections/{connection}/test` | manage | — | `{"data":Connection}` (with new `last_probe`) | 404 |
| POST `/workspace/{workspace}/integrations/connections/{connection}/disconnect` | manage | — | `{"message":"ok"}` | 404 |
| GET `/workspace/{workspace}/integrations/pairs` | view | — | list of `Pair` | |
| POST `/workspace/{workspace}/integrations/pairs` | manage | `PairInput` | 201 `{"data":{"pair_id"}}` | 422 (wrong provider per side) |
| GET/POST `/workspace/{workspace}/integrations/pairs/{pair}` | view / manage | — / `PairInput` patch | `{"data":Pair}` / `{"message":"ok"}` | 404 |
| POST `/workspace/{workspace}/integrations/pairs/{pair}/activate` | manage | — | `{"data":Pair}` | 422 `connection_not_active`, 422 `source_controls_unmet` (detail lists companies) |
| POST `/workspace/{workspace}/integrations/pairs/{pair}/pause` | manage | — | `{"data":Pair}` | |
| GET `/workspace/{workspace}/integrations/pairs/{pair}/mappings?entity=&page=&page_size=` | view | — | list of `Mapping` | 422 unknown entity |
| POST `/workspace/{workspace}/integrations/pairs/{pair}/mappings` | manage | `{entity, source_id, target_id, source_label?, target_label?}` upsert by `(entity, source_id)` | 201 `{"data":{"mapping_id"}}` | 422 (`user` target not an active member) |
| DELETE `/workspace/{workspace}/integrations/pairs/{pair}/mappings/{mapping}` | manage | — | `{"message":"ok"}` | 404 |

Delete uses HTTP `DELETE`, as `src/modules/currencies/handlers.rs:145` does.

JSON:
```jsonc
// Connection
{ "id": "uuid", "provider": "odoo|netsuite", "label": "…", "status": "draft|active|degraded|unauthorized|expired|disconnected",
  "status_reason": "code|null", "config": { … §4.5 … }, "secrets": { "api_key": "********" | null },   // null = none stored
  "credential_expires_at": "rfc3339|null", "last_probe": [CapabilityCheck] | null, "last_probe_at": "rfc3339|null",
  "created_at": "rfc3339", "updated_at": "rfc3339" }
// ConnectionInput
{ "provider": "odoo|netsuite" /* create only */, "label": "…", "config": { … }, "secrets": { "api_key"|"private_key": "plain|********" },
  "credential_expires_at": "rfc3339|null" }
// CapabilityCheck
{ "key": "purchase_request.read", "ok": true, "error_class": "auth|…|null", "detail": "readable|null" }
// Pair
{ "id", "workflow_key": "wf01_purchase", "source_connection_id", "target_connection_id", "status": "draft|active|paused",
  "paused_reason": "code|null", "config": { … §2.1 … }, "activated_at", "last_polled_at", "created_at", "updated_at" }
// Mapping
{ "id", "pair_id", "entity", "source_id", "target_id", "source_label", "target_label", "updated_at" }
```

### 5.3 Approvals (C2)

| method + path | permission | body | success | errors |
|---|---|---|---|---|
| GET `/workspace/{workspace}/approvals/requests?workflow_key=&status=&scope=all\|mine\|decidable&page=&page_size=` | member; `scope=all` needs any of view/approve/approve_high/reconcile/manage; without it the scope is forced to `mine` | — | list of `ApprovalRequest` (summary, no `snapshot.lines`) | 422 unknown status |
| GET `/workspace/{workspace}/approvals/requests/{request}` | same visibility rule, else 404 | — | `{"data":ApprovalRequest}` full | 404 |
| POST `/workspace/{workspace}/approvals/requests/{request}/approve` | row rule §3.1 | `{"fingerprint":"64hex","comment":string?}` + `Idempotency-Key` | `{"data":ApprovalRequest}` | 403 `four_eyes_required`, 403, 404, 409 `approval_already_decided`, 409 `approval_stale` |
| POST `/workspace/{workspace}/approvals/requests/{request}/reject` | same | `{"fingerprint","comment"}` (comment required) | same | + 422 comment |
| POST `/workspace/{workspace}/approvals/requests/{request}/return` | same | same | same | same |
| POST `/workspace/{workspace}/approvals/requests/{request}/resubmit` | the requester or `integration.manage` | `{"comment":string?}` + `Idempotency-Key` | 201 `{"data":{"approval_request_id"}}` | 409 `approval_not_resubmittable` (latest is not rejected/returned, or not the latest revision) |
| GET `/workspace/{workspace}/approvals/policies` | view | — | list of `Policy` | |
| POST `/workspace/{workspace}/approvals/policies` | manage | `{workflow_key, currency_ref, threshold_minor}` | 201 `{"data":{"policy_id"}}` | 422 duplicate active |
| POST `/workspace/{workspace}/approvals/policies/{policy}` | manage | `{threshold_minor?, active?}` | `{"message":"ok"}` | |

`scope=decidable` = pending requests where the caller passes §3.1 checks 1-2.

```jsonc
// ApprovalRequest
{ "id", "workflow_key": "wf01_purchase", "pair_id", "subject_ref", "revision": 1, "fingerprint": "64hex",
  "status": "blocked|pending|approved|rejected|returned|invalidated",
  "blocked_reasons": [ { "code": "unmapped", "entity": "vendor", "source_id": "42" } ],  // parsed from text[]
  "invalidated_reason": "code|null",
  "amount": { "minor": 123456, "currency_ref": "EUR", "decimals": 2 },
  "required_permission": "integration.approve|integration.approve_high|null",
  "requester": { "user_id", "name" } | null,
  "decision": { "decision": "approve|reject|return", "actor": { "user_id", "name" }, "comment", "created_at" } | null,
  "can_decide": true,                                   // §3.1 checks 1-2 for the caller, so the UI hides buttons
  "execution": ExecutionSummary | null,                 // separate from approval status (PLAN test 23)
  "open_incidents": [ { "id", "kind" } ],
  "snapshot": Snapshot,                                 // detail only
  "created_at", "updated_at" }
```

### 5.4 Snapshot JSON v1 (written by C4a, read by C2, C4b and C5)

```jsonc
{ "v": 1,
  "source": { "connection_id", "model": "purchase.order", "id": "17", "name": "P00017" | null, "state": "draft|sent" },
  "target": { "connection_id", "record_type": "purchaseOrder" },
  "company":  { "source_id", "target_id", "label" },     // target_id = subsidiary
  "vendor":   { "source_id", "target_id", "label" },
  "currency": { "source_id", "target_id", "ref": "EUR", "decimals": 2 },
  "location": { "source_id": null, "target_id", "label" } | null,
  "requester": { "source_id", "user_id" } | null,
  "lines": [ { "source_line_id", "product": { "source_id", "target_id", "label" }, "uom": { "source_id", "target_id", "label" },
               "quantity": "3.00000000", "price_unit": "0.33300000", "subtotal_minor": 100, "date_planned": "…" | null } ],
  "amount_untaxed_minor": 100, "amount_tax_minor": 21, "amount_total_minor": 121,
  "tax_note": "odoo_estimate_target_calculates",
  "mapping_ids": { "<entity>:<source_id>": "<mapping uuid>" } }
```
- A `target_id` is `null` only when the request is `blocked`.
- Decimals are strings (K25). `label` may be null (source display names are per R1).

### 5.5 Workflows (C2 routes; C4b fills `link_existing`)

| method + path | permission | body | success | errors |
|---|---|---|---|---|
| GET `/workspace/{workspace}/workflows/executions?workflow_key=&status=&page=&page_size=` | view | — | list of `Execution` | |
| GET `/workspace/{workspace}/workflows/executions/{execution}` | view | — | `{"data":Execution}` + `attempts`, `events`, `resolutions` | 404 |
| POST `/workspace/{workspace}/workflows/executions/{execution}/retry` | manage | `{"reason"}` + `Idempotency-Key` | `{"data":Execution}` | 409 `execution_state_changed` (not `blocked`) |
| POST `/workspace/{workspace}/workflows/executions/{execution}/resolve` | reconcile, not the requester | `{"action":"link_existing\|allow_retry\|close_no_po\|accept_variance\|reject_variance","reason","target_id"?}` + `Idempotency-Key` | `{"data":Execution}` | 403 `four_eyes_required`, 409 `execution_state_changed`, 422 `target_id` required for link, 503 `link_unavailable` until C4b |
| GET `/workspace/{workspace}/workflows/incidents?status=&kind=&page=&page_size=` | view | — | list of `Incident` | |
| POST `/workspace/{workspace}/workflows/incidents/{incident}/resolve` | reconcile, not the requester | `{"note"}` + `Idempotency-Key` | `{"data":Incident}` | 409 `incident_already_resolved` |

```jsonc
// ExecutionSummary
{ "id", "status": "queued|running|blocked|awaiting_reconciliation|cancelled|succeeded|succeeded_with_variance|resolved_manual|failed",
  "phase": "prechecking|write_intent|write_sent|verifying|done", "reason": "code|null", "resolution": "…|null",
  "error_class": "…|null", "error_message": "readable|null", "target_ref": "…|null", "verified": true|false, "updated_at" }
// Execution = ExecutionSummary + { "approval_request_id", "workflow_key", "correlation_id", "remote_attempts", "manual_retry_grants",
//   "allowed_actions": ["allow_retry", …] , "created_at" }
// detail adds: "attempts": [{ "attempt_no","granted_by","grant_reason","write_intent_at","sent_at","outcome" }],
//   "events": [{ "kind","actor_kind","created_at","summary" }], "resolutions": [{ "action","actor","reason","outcome","created_at" }]
// Incident
{ "id", "kind", "subject_ref", "status": "open|resolved", "pair_id","approval_request_id","execution_id", "detail": {…}, "note", "resolved_by", "created_at" }
```
`verified` is true only for `succeeded` and `succeeded_with_variance`, never for `resolved_manual` (R3-§6c).

### 5.6 Error code registry (all `AppError::Reason`, K14)

| status | code | where |
|---|---|---|
| 403 | `four_eyes_required` | decide, resolve, incident resolve |
| 409 | `approval_already_decided` | decide |
| 409 | `approval_stale` | decide |
| 409 | `approval_not_resubmittable` | resubmit |
| 409 | `execution_state_changed` | retry, resolve |
| 409 | `incident_already_resolved` | incident resolve |
| 409 | `retry_needs_grant` | retry (`retry_after_fix`) when the latest attempt has an outcome and no unused grant (§10) |
| 409 | `retry_already_granted` | resolve `allow_retry` while a grant is unused (`remote_attempts <= manual_retry_grants`); attempt n uses grant n-1 |
| 422 | `connection_not_active` | pair activate |
| 422 | `source_controls_unmet` | pair activate |
| 503 | `link_unavailable` | resolve `link_existing` before C4b merges |

The UI maps each code to a readable sentence; raw codes are never shown on screen.

### 5.7 Example responses (fixtures for C5's fake API)

Sample data. Ids are shortened placeholders (`c1…` = uuid). Error bodies follow K14 exactly. Every route lists one success and one error.

**Connections**
```jsonc
// GET /workspace/w1/integrations/connections → 200
{"data":[{"id":"c1","provider":"odoo","label":"Odoo main","status":"active","status_reason":null,
  "config":{"base_url":"https://acme.odoo.com","database":null},"secrets":{"api_key":"********"},
  "credential_expires_at":"2027-02-01T00:00:00Z","last_probe":[{"key":"purchase_request.read","ok":true,"error_class":null,"detail":null}],
  "last_probe_at":"2026-11-17T10:00:00Z","created_at":"2026-11-17T09:00:00Z","updated_at":"2026-11-17T10:00:00Z"}],
 "meta":{"current_page":1,"from":1,"last_page":1,"per_page":50,"to":1,"total":1}}
// → 403 (no integration.view)
{"message":"This action is unauthorized."}

// POST /workspace/w1/integrations/connections → 201
{"data":{"connection_id":"c2"}}
// → 422
{"message":"The label field is required. (and 1 more error)","errors":{"config.base_url":["The base url must be a valid URL."],"label":["The label field is required."]}}

// GET /workspace/w1/integrations/connections/c1 → 200 {"data":{…Connection as above…}}
// → 404 (unknown or other workspace)
{"message":"The requested resource could not be found."}

// POST /workspace/w1/integrations/connections/c1 → 200
{"message":"ok"}
// → 422 (mask sent after base_url changed)
{"message":"Enter this credential again.","errors":{"secrets.api_key":["Enter this credential again."]}}

// POST /workspace/w1/integrations/connections/c3/test → 200
{"data":{"id":"c3","provider":"netsuite","label":"NS prod","status":"unauthorized","status_reason":"auth_failed",
  "config":{"rest_domain":"1234567.suitetalk.api.netsuite.com","environment":"production","client_id":"abc","certificate_id":"kid1","alg":"ES256"},
  "secrets":{"private_key":"********"},"credential_expires_at":"2028-11-01T00:00:00Z",
  "last_probe":[{"key":"auth.token","ok":false,"error_class":"auth","detail":"The token request was refused. Check the certificate and client id."}],
  "last_probe_at":"2026-11-17T10:05:00Z","created_at":"2026-11-17T09:00:00Z","updated_at":"2026-11-17T10:05:00Z"}}
// → 404 {"message":"The requested resource could not be found."}

// POST /workspace/w1/integrations/connections/c1/disconnect → 200 {"message":"ok"}
// → 403 {"message":"This action is unauthorized."}
```

**Pairs and mappings**
```jsonc
// GET /workspace/w1/integrations/pairs → 200
{"data":[{"id":"p1","workflow_key":"wf01_purchase","source_connection_id":"c1","target_connection_id":"c3","status":"paused",
  "paused_reason":"odoo_control_drift","config":{"poll_minutes":5,"backfill_days":0,"full_scan_minutes":60,"succeeded_watch_days":30,
  "tax_tolerance_minor":0,"default_locations":{"1":"7"}},"activated_at":"2026-11-17T10:10:00Z","last_polled_at":"2026-11-17T11:00:00Z",
  "created_at":"2026-11-17T10:00:00Z","updated_at":"2026-11-17T11:00:00Z"}],
 "meta":{"current_page":1,"from":1,"last_page":1,"per_page":50,"to":1,"total":1}}
// → 403 {"message":"This action is unauthorized."}

// POST /workspace/w1/integrations/pairs → 201 {"data":{"pair_id":"p1"}}
// → 422 {"message":"The selected source connection id is invalid.","errors":{"source_connection_id":["The selected source connection id is invalid."]}}

// GET /workspace/w1/integrations/pairs/p1 → 200 {"data":{…Pair…}}   → 404 as above
// POST /workspace/w1/integrations/pairs/p1 → 200 {"message":"ok"}    → 422 {"message":"The poll minutes must not be greater than 1440.","errors":{"config.poll_minutes":["The poll minutes must not be greater than 1440."]}}

// POST /workspace/w1/integrations/pairs/p1/activate → 200 {"data":{…Pair, "status":"active","paused_reason":null…}}
// → 422
{"message":"Odoo purchase approval settings do not meet the requirement for: Acme BV.","code":"source_controls_unmet"}
// → 422
{"message":"Both connections must be active first.","code":"connection_not_active"}

// POST /workspace/w1/integrations/pairs/p1/pause → 200 {"data":{…Pair, "status":"paused","paused_reason":"manual"…}}
// → 404 {"message":"The requested resource could not be found."}

// GET /workspace/w1/integrations/pairs/p1/mappings?entity=vendor → 200
{"data":[{"id":"m1","pair_id":"p1","entity":"vendor","source_id":"42","target_id":"1187","source_label":"Azure Interior","target_label":"Azure Interior Ltd","updated_at":"2026-11-17T10:20:00Z"}],
 "meta":{"current_page":1,"from":1,"last_page":1,"per_page":50,"to":1,"total":1}}
// → 422 {"message":"The selected entity is invalid.","errors":{"entity":["The selected entity is invalid."]}}

// POST /workspace/w1/integrations/pairs/p1/mappings → 201 {"data":{"mapping_id":"m1"}}
// → 422 {"message":"The selected target id is invalid.","errors":{"target_id":["The selected target id is invalid."]}}

// DELETE /workspace/w1/integrations/pairs/p1/mappings/m1 → 200 {"message":"ok"}   → 404 as above
```

**Approvals**
```jsonc
// GET /workspace/w1/approvals/requests?scope=decidable → 200 (list rows omit "snapshot")
{"data":[{"id":"r1","workflow_key":"wf01_purchase","pair_id":"p1","subject_ref":"odoo:c1:purchase.order:17","revision":1,
  "fingerprint":"9f2c…64hex","status":"pending","blocked_reasons":[],"invalidated_reason":null,
  "amount":{"minor":125000,"currency_ref":"EUR","decimals":2},"required_permission":"integration.approve",
  "requester":{"user_id":"u1","name":"Ana Buyer"},"decision":null,"can_decide":true,"execution":null,"open_incidents":[],
  "created_at":"2026-11-17T11:00:00Z","updated_at":"2026-11-17T11:00:00Z"}],
 "meta":{"current_page":1,"from":1,"last_page":1,"per_page":50,"to":1,"total":1}}
// → 422 {"message":"The selected status is invalid.","errors":{"status":["The selected status is invalid."]}}

// GET /workspace/w1/approvals/requests/r2 → 200 (blocked, full)
{"data":{"id":"r2","workflow_key":"wf01_purchase","pair_id":"p1","subject_ref":"odoo:c1:purchase.order:18","revision":1,"fingerprint":"1a7e…64hex",
  "status":"blocked","blocked_reasons":[{"code":"unmapped","entity":"vendor","source_id":"43"},{"code":"unmapped","entity":"product","source_id":"9"}],
  "invalidated_reason":null,"amount":{"minor":30000,"currency_ref":"EUR","decimals":2},"required_permission":null,
  "requester":{"user_id":"u1","name":"Ana Buyer"},"decision":null,"can_decide":false,"execution":null,"open_incidents":[],
  "snapshot":{"v":1,"source":{"connection_id":"c1","model":"purchase.order","id":"18","name":"P00018","state":"draft"},
    "target":{"connection_id":"c3","record_type":"purchaseOrder"},
    "company":{"source_id":"1","target_id":"3","label":"Acme BV"},"vendor":{"source_id":"43","target_id":null,"label":"Deco Addict"},
    "currency":{"source_id":"1","target_id":"1","ref":"EUR","decimals":2},"location":null,"requester":{"source_id":"2","user_id":"u1"},
    "lines":[{"source_line_id":"55","product":{"source_id":"9","target_id":null,"label":"Desk"},"uom":{"source_id":"1","target_id":"1","label":"Units"},
      "quantity":"2.00000000","price_unit":"150.00000000","subtotal_minor":30000,"date_planned":null}],
    "amount_untaxed_minor":30000,"amount_tax_minor":6300,"amount_total_minor":36300,"tax_note":"odoo_estimate_target_calculates",
    "mapping_ids":{"company:1":"m7","currency:1":"m8","uom:1":"m9","user:2":"m10"}},
  "created_at":"2026-11-17T11:00:00Z","updated_at":"2026-11-17T11:00:00Z"}}
// → 404 {"message":"The requested resource could not be found."}

// POST /workspace/w1/approvals/requests/r1/approve  (Idempotency-Key: k1)  body {"fingerprint":"9f2c…"}
// → 200 {"data":{…r1, "status":"approved","can_decide":false,
//   "decision":{"decision":"approve","actor":{"user_id":"u2","name":"Ben Approver"},"comment":null,"created_at":"2026-11-17T11:05:00Z"},
//   "execution":{"id":"e1","status":"queued","phase":"prechecking","reason":null,"resolution":null,"error_class":null,"error_message":null,"target_ref":null,"verified":false,"updated_at":"2026-11-17T11:05:00Z"}}}
// → 409
{"message":"This request changed after you opened it. Reload to see the new version.","code":"approval_stale"}
// → 409
{"message":"This request was already decided.","code":"approval_already_decided"}
// → 403
{"message":"You cannot decide a request you raised.","code":"four_eyes_required"}

// POST …/r1/reject  body {"fingerprint":"9f2c…","comment":"Wrong vendor"} → 200 {"data":{…"status":"rejected"…}}
// → 422 {"message":"The comment field is required.","errors":{"comment":["The comment field is required."]}}

// POST …/r1/return  → 200 {"data":{…"status":"returned"…}}   → 409 approval_already_decided as above

// POST /workspace/w1/approvals/requests/r3/resubmit → 201 {"data":{"approval_request_id":"r4"}}
// → 409
{"message":"Only the latest rejected or returned request can be resubmitted.","code":"approval_not_resubmittable"}

// GET /workspace/w1/approvals/policies → 200
{"data":[{"id":"pol1","workflow_key":"wf01_purchase","currency_ref":"EUR","threshold_minor":500000,"active":true,"created_at":"2026-11-17T09:30:00Z","updated_at":"2026-11-17T09:30:00Z"}],
 "meta":{"current_page":1,"from":1,"last_page":1,"per_page":50,"to":1,"total":1}}
// → 403 {"message":"This action is unauthorized."}
// POST /workspace/w1/approvals/policies → 201 {"data":{"policy_id":"pol1"}}
// → 422 {"message":"An active policy already exists for this currency.","errors":{"currency_ref":["An active policy already exists for this currency."]}}
// POST /workspace/w1/approvals/policies/pol1 → 200 {"message":"ok"}  → 404 as above
```

**Workflows**
```jsonc
// GET /workspace/w1/workflows/executions?status=awaiting_reconciliation → 200
{"data":[{"id":"e2","approval_request_id":"r5","workflow_key":"wf01_purchase","correlation_id":"vz-0f1e2d3c4b5a69788796a5b4c3d2e1f0",
  "status":"awaiting_reconciliation","phase":"write_sent","reason":null,"resolution":null,"error_class":"ambiguous",
  "error_message":"NetSuite did not answer in time. Flow is checking whether the order was created.","target_ref":null,"verified":false,
  "remote_attempts":1,"manual_retry_grants":0,"allowed_actions":["link_existing","close_no_po"],
  "created_at":"2026-11-17T11:10:00Z","updated_at":"2026-11-17T11:12:00Z"}],
 "meta":{"current_page":1,"from":1,"last_page":1,"per_page":50,"to":1,"total":1}}
// → 403 {"message":"This action is unauthorized."}

// GET /workspace/w1/workflows/executions/e2 → 200 {"data":{…as above…,
//   "attempts":[{"attempt_no":1,"granted_by":null,"grant_reason":null,"write_intent_at":"2026-11-17T11:11:00Z","sent_at":"2026-11-17T11:11:01Z","outcome":"ambiguous"}],
//   "events":[{"kind":"write_intent","actor_kind":"system","created_at":"2026-11-17T11:11:00Z","summary":"Ready to create the order"}],
//   "resolutions":[]}}
// → 404 {"message":"The requested resource could not be found."}

// POST /workspace/w1/workflows/executions/e3/retry  body {"reason":"Vendor mapped"} → 200 {"data":{…"status":"queued"…}}
// → 409
{"message":"This run changed state. Reload and try again.","code":"execution_state_changed"}

// POST /workspace/w1/workflows/executions/e2/resolve  body {"action":"link_existing","reason":"Found in NetSuite","target_id":"8812"}
// → 200 {"data":{…"status":"resolved_manual","resolution":"linked_with_differences","target_ref":"netsuite:c3:purchaseOrder:8812","verified":false…}}
// → 403 {"message":"You cannot resolve a request you raised.","code":"four_eyes_required"}
// → 503 (before C4b) {"message":"Linking an existing order is not available yet.","code":"link_unavailable"}

// GET /workspace/w1/workflows/incidents?status=open → 200
{"data":[{"id":"i1","kind":"duplicate_obligation_risk","subject_ref":"odoo:c1:purchase.order:17","status":"open","pair_id":"p1",
  "approval_request_id":"r1","execution_id":"e1","detail":{"source_state":"purchase"},"note":null,"resolved_by":null,"created_at":"2026-11-17T12:00:00Z"}],
 "meta":{"current_page":1,"from":1,"last_page":1,"per_page":50,"to":1,"total":1}}
// → 403 {"message":"This action is unauthorized."}

// POST /workspace/w1/workflows/incidents/i1/resolve  body {"note":"Odoo PO cancelled by hand"} → 200 {"data":{…"status":"resolved","note":"Odoo PO cancelled by hand","resolved_by":"u3"…}}
// → 409 {"message":"This incident is already resolved.","code":"incident_already_resolved"}
```
`code`, status and JSON keys are frozen. `message` text is sample wording: C1/C2 may reword it and tell the parent, who updates these fixtures; C5's UI keys off `code` and status, never `message`.

---

## 6. Jobs (`src/jobs/kinds.rs`: **C4a only**)

`SCHEDULES` are global and fixed-interval (K16). Per-pair intervals therefore use one fan-out tick.

| kind const | name | added by | enqueue | keys |
|---|---|---|---|---|
| `WF01_TICK` | `wf01_tick` | C4a | `SCHEDULES` `Every::Minutes(1)` | system job; enqueues the due per-pair jobs below |
| `WF01_POLL` | `wf01_poll` | C4a | tick, when `last_polled_at + poll_minutes ≤ now` | ws job, `dedupe_key = "wf01_poll:<pair>"`, `concurrency_key = "wf01:<pair>"` |
| `WF01_SWEEP` | `wf01_sweep` | C4a | tick, same cadence | `dedupe_key = "wf01_sweep:<pair>"`, `concurrency_key = "wf01:<pair>"` |
| `WF01_FULL_SCAN` | `wf01_full_scan` | C4a | tick, `full_scan_minutes` | `dedupe_key = "wf01_full_scan:<pair>"`, `concurrency_key = "wf01:<pair>"` |
| `WF01_EXECUTE` | `wf01_execute` | C4a (body C4b) | `dispatch::enqueue_execution` + tick backstop for `queued` rows | `dedupe_key = <execution_id>`, `max_attempts = 5` |
| `WF01_RECONCILE` | `wf01_reconcile` | C4a (body C4b) | execute, on entering `awaiting_reconciliation` | `dedupe_key = "wf01_reconcile:<execution_id>"`, `max_attempts = 10` |

- The shared `concurrency_key` `wf01:<pair>` means poll, sweep and full scan never run at the same time for one pair (K17: unique across workspaces; the pair uuid makes it unique).
- `SCHEDULES: [Schedule; 10]` becomes `[Schedule; 11]`.
- C4a declares **all** kinds above, each with a `dispatch` arm calling `crate::workflows::wf01_purchase::<step>::run(ctx, &job)` (poll, sweep, full_scan, tick, execute, reconcile).
- `execute.rs` and `reconcile.rs` start in C4a as stubs whose `run` returns a permanent `AppError` (e.g. `AppError::Unavailable("wf01 execute not built")`). C4b replaces only their bodies and never touches `kinds.rs`. Stubs live on the branch only; they are never merged to main as stubs.
- `wf01_tick` is the parent's `SCHEDULES` addition to the plan's five kinds (needed because K16 schedules are global); it is C4a's.

---

## 7. UI contract (C5)

- **Shared components** in `packages/ui/src/components/business-approvals/`, imported as `@verzo/ui/components/business-approvals/<file>` (K24; O-8):
  - `purchase-approval-card.tsx` → `PurchaseApprovalCard({ request: ApprovalRequestView, onApprove?(comment?: string), onReject?(comment: string), onReturn?(comment: string), busy?: boolean, error?: string | null, mappingHref?: (b: BlockedReasonView) => string | undefined })`. It renders amount, untaxed label, the "Odoo estimate; NetSuite calculates tax" line, lines, status, blocked reasons (with links via `mappingHref`), and `ExecutionStatus`. Buttons show only when `request.can_decide`.
  - `approval-list.tsx` → `ApprovalList({ items: ApprovalRequestView[], selectedId?: string, onSelect(id: string), empty: React.ReactNode })`. It uses `EmptyState` for empty.
  - `execution-status.tsx` → `ExecutionStatus({ execution: ExecutionView | null, onAction?(action: ResolveAction | "retry", reason: string, targetId?: string), busy?: boolean })`. Approval status and execution status are shown separately. "Verified" shows only when `execution.verified`.
  - `incident-badge.tsx` → `IncidentBadge({ kind: IncidentKind, status: "open" | "resolved" })`.
  - `types.ts` → `ApprovalRequestView`, `BlockedReasonView`, `ExecutionView`, `ResolveAction`, `IncidentKind`, mirroring §5.3/§5.5 exactly. Replace with generated types (K23) when C1 and C2 merge.
  - `labels.ts` → readable text for every status, reason, error and incident code in §3 and §5.6. There are no raw codes on screen.
- **States:** every screen handles empty, error, info, warning and success via `Notice` / `EmptyState` (K24).
- **Routes:** `apps/flow/app/(app)/approvals/purchases/page.tsx` and `apps/office/app/(app)/approvals/purchases/page.tsx`. Office settings: `apps/office/app/(app)/settings/integrations/page.tsx` (connections + test), `…/settings/integrations/[pair]/mappings/page.tsx`.
- **Forbidden:** `apps/flow/components/task/approval-scene.tsx` and `apps/flow/components/frame/dock.tsx` are not edited, imported or renamed. T37 asserts the agent-task approval scene is unchanged.
- **Mocks:** C5-mocked uses the `packages/e2e/src/api-mock.ts` layer with fixtures matching §5 JSON byte for byte. Decide calls send `Idempotency-Key` via `useIdempotencyKey()` (K15).

---

## 8. Ownership matrix

| file / area | owner | others |
|---|---|---|
| `migrations/<ts>_integrations.*` | C1 | — |
| `migrations/<ts>_approvals_workflows.*` | C2 | — |
| `src/integrations/**` except `providers/odoo`, `providers/netsuite` | C1 | C3a/C3b add one variant + arms in `providers/mod.rs`; C4a adds the activation-check call in `pairs/service.rs` (one call site) |
| `src/integrations/providers/odoo/**` | C3a | — |
| `src/integrations/providers/netsuite/**` | C3b | — |
| `src/modules/approvals/**`, `src/workflows/{mod,executions,events,incidents,dispatch,routes,handlers,dto}.rs` | C2 | C4b edits the `dispatch.rs` arm and the `link_existing` call in the resolve service |
| `src/workflows/wf01_purchase/{poll,sweep,full_scan,fingerprint,evaluate,activation}.rs` | C4a | — |
| `src/workflows/wf01_purchase/{execute,reconcile}.rs` | C4a creates stubs | C4b replaces bodies |
| `src/workflows/wf01_purchase/{verify,link}.rs` | C4b | — |
| `src/modules/permissions/catalog.rs`, `tests/api/permissions.rs:74` | **C1 only** | — |
| `src/jobs/kinds.rs` | **C4a only** (all WF-01 kinds) | — |
| `src/modules/settings/credentials.rs` `PATHS` + credentials CHECK | **C1 only**, and per O-1 C1 does not need to touch them (`integration_secrets`) | — |
| `src/lib.rs` (`mod` line), `src/modules/mod.rs` (`pub mod approvals;`, `src/modules/mod.rs:3-4` style) | C1 (`integrations`), C2 (`workflows`, `approvals`) | C1 and C2 run **in parallel**; each adds its own lines, no sequencing; parent resolves the merge |
| `src/openapi.rs` (`.merge(...)` lines) | C1 (integrations routes), C2 (approvals + workflows routes) | parallel; parent resolves the merge |
| `tests/api/main.rs` (`mod` lines) | each child adds its own test files | parent merges |
| `apps/api/openapi.json`, `packages/api-client/src/generated/**` | regenerated by C1 and C2 in their branches; **parent re-runs regen on the merged tip**, never hand-merges | — |
| `apps/api/CHANGELOG.md`, `apps/api/docs/api/{integrations,approvals,workflows}.md` | C1 → integrations.md; C2 → approvals.md + workflows.md; CHANGELOG lines each | parent merges |
| `Cargo.toml` (`ring` direct dep, R3-D10) | **C3b only** | — |
| `packages/ui/src/components/business-approvals/**`, `apps/{flow,office}/app/(app)/approvals/**`, `apps/office/app/(app)/settings/integrations/**`, their e2e | C5 | — |
| nav links in `apps/flow` / `apps/office` layouts | C5 | — |
| `modules/purchasing/**` | **nobody** (M3) | — |
| `docs/integrations/facts/*` | R1 | — |
| `docs/integrations/contracts.md` | parent | — |

---

## 9. OPEN items

| id | item | who resolves |
|---|---|---|
| O-1 | **D6 decided by evidence, parent to confirm.** `PATHS` is a fixed list pinned by a count test (`src/modules/settings/credentials.rs:16-31,45-56`), and `workspace_credentials` has PK `(workspace_id, path)` (`migrations/20261021151609_credentials.up.sql:21`). Per-connection secrets would need a CHECK pattern and a rewrite of that test. This contract therefore uses the D6 fallback `integration_secrets`. | parent |
| O-2 | `required_permission` lives on `approval_requests` and policies hold only `threshold_minor`. This replaces `approval_policies.approver_permission` from PLAN §4, which conflicts with D4 "threshold selects approve vs approve_high". `approve_high` also satisfies `approve`. | parent |
| O-3 | `currency_ref` = ISO 4217 code from the source currency. Odoo `res.currency.name` as the ISO code is unverified (R1 O2 family). Until R1 confirms it, the column stays `text(1..64)` with no ISO CHECK. | R1 → parent |
| O-4 | `blocked_reasons` stays `text[]` per PLAN, and the API parses it into objects. | parent (accepted unless objected) |
| O-5 | The snapshot `label` fields need source display names; whether JSON-2 `read` returns many2one `[id, name]` pairs is unverified. | R1 |
| O-8 | `@verzo/ui` export `"./components/*"` with a nested folder: there is no nested component folder yet (`packages/ui/src/components` has none). Node subpath `*` matches `/`, but this is unproven in this repo's TS/Next setup. C5 proves it with a build, or falls back to flat names `business-approvals-*.tsx` and asks the parent. | C5 |
| O-9 | Whether `wf01_tick` should be a system job (no workspace) that enqueues workspace jobs: `NewJob.workspace_id` allows `None` (`src/jobs/repo.rs:23-24`), but no existing system job fans out into workspace jobs. | C4a |
| O-10 | P1/P2 money checks: per R1 O6 (PLAN P-3). Codes `rate_precision`, `untotalled_lines` frozen; formula not. C4a/C4b may not implement P1/P2 before O6. | R1 → parent |
| O-11 | Every NetSuite/Odoo field, sublist, error code and limit: "per R1" (`docs/integrations/facts/*`); LIVE gates L1-L6 remain. | R1, LIVE |

## 10. Parent decisions on OPEN items (2026-11-17)

- O-1: use the `integration_secrets` table, sealed with `CredentialKey` and path `integration:<connection_id>:<name>`. Do not touch `PATHS`.
- O-2: accepted. `required_permission` lives on `approval_requests`. `approve_high` also satisfies `approve`.
- O-3, O-5, O-10, O-11: wait for R1. C1/C2 keep these fields as opaque text.
- O-4: accepted.
- O-8 and O-9: C5 and C4a must each prove their item or report back before choosing a fallback.
- `wf01_tick` fan-out: accepted.
- C1's catalog commit goes first and stands alone; the parent cherry-picks it onto C2.
- Correction to §3.2: after a definite `rejected` attempt, `retry_after_fix` alone cannot start a new attempt, because the attempt cap holds. The UI must lead the user to `allow_retry` (the `integration.reconcile` permission, non-requester, reason required), which grants the extra attempt. C2 enforces this: `retry_after_fix` on an execution whose latest attempt has a non-NULL outcome and no unused grant → 409 `retry_needs_grant`.

---

## 11. WF-02..WF-07 frozen names (B0a, 2026-11-21)

Status: **frozen** for build children B1.1, B1.2, B2.1, B2.2, B3 and B4. Source: `docs/integrations/PLAN-WF02-07.md` rev 1 (approved 2026-11-21, defaults D-21..D-74). The §1 conventions K1..K25 and the change rule at the top of this file apply unchanged. Where the plan is silent or inconsistent, the choice is listed in §11.15 ("Resolved by B0a").

- **Version:** `contracts v1.2 (2026-11-21)`: v1.1 plus §11.
- **Money rule (once for all of §11):** money and quantity columns are `bigint` minor units where the name ends in `_minor`, otherwise Postgres `numeric` (exact; provider decimals parsed via `serde_json` arbitrary_precision → `rust_decimal`, bound as `Decimal`, rendered as JSON strings per K25). Every sum, extension, variance, tolerance, threshold, estimate and rounding is computed in SQL. Rust only parses and binds; frontends only display.
- **Table rules:** as §2 (K1 id, K2 `workspace_id` + `<t>_workspace_id_id_key`, composite FKs `(workspace_id, x_id) → x (workspace_id, id)`, `created_at`, `updated_at` on mutable tables, `text(a..b)` = `char_length` CHECK, AO = append-only per K5). Not repeated per table below.
- **Paths:** relative to `/v1` (K9), `{workspace}` as in §5. Every route gets a cross-workspace 404 test (K11). Every route's `operation_id` is listed; it is `disposition: "internal"` in `operation-policy.json` unless §11.12 names it as a Verzo tool.
- **Permissions:** no new keys (catalog stays at 263). `view` = `integration.view`, `reconcile` = `integration.reconcile`, `approve` = `integration.approve`, `manage` = `integration.manage`.

### 11.1 Migrations (one per build child, reversible, K8)

| wave | child | file | creates | widens |
|---|---|---|---|---|
| B1.1 | WF-03 + X4/X5/X6 | `<ts>_wf03_recovery.{up,down}.sql` | `exchange_expectations` | `workflow_incidents` (kind, new columns, resolve CHECKs, §11.3) |
| B1.2 | WF-05 + X1/X2/X7 | `<ts>_wf05_reports.{up,down}.sql` | `ext_read_runs`, `ext_records`, `report_join_rules`, `report_definitions`, `report_runs`, `report_rows` | `workflow_key` += `wf05_report`; `scheduled_reports.report` += `cross_erp_exceptions` |
| B2.1 | WF-02 + X3 | `<ts>_wf02_match.{up,down}.sql` | `review_cases`, `review_case_decisions`, `match_tolerances`, `match_runs`, `match_lines` | `workflow_key` += `wf02_match` |
| B2.2 | WF-04 | `<ts>_wf04_stock.{up,down}.sql` | `stock_location_scopes`, `stock_checks`, `stock_check_lines` | `workflow_key` += `wf04_stock`; `review_cases.workflow_key` += `wf04_stock`; `integration_mappings.entity` += `stock_location` |
| B3 | WF-06 | `<ts>_wf06_replenish.{up,down}.sql` | `replenish_proposals` | `workflow_key` += `wf06_replenish`; `exchange_expectations.workflow_key` += `wf06_replenish` |
| B4 | WF-07 | `<ts>_wf07_definitions.{up,down}.sql` | `workflow_definitions`, `workflow_definition_versions`, `workflow_runs`, `workflow_run_steps` | `workflow_key` += `wf07_custom`; `exchange_expectations.workflow_key` += `wf07_custom`; `workflow_incidents.kind` += `waiting_data`; `integration_mappings.entity` += `customer` |

- "`workflow_key` +=" (X6) means the same value is added to the CHECK on **all four** of `integration_pairs`, `approval_policies`, `approval_requests`, `workflow_executions`. Lists are cumulative; each `down.sql` restores the previous list.
- `<ts>` is later than the latest migration on the merged tip when the child starts (base: `20261119210000`). Order follows the wave order; the parent renumbers if needed.
- B0b (facts) has no migration.

Final `workflow_key` list after B4: `('wf01_purchase','wf05_report','wf02_match','wf04_stock','wf06_replenish','wf07_custom')`. WF-03 adds no key (it watches other workflows' writes).

### 11.2 X1 + X2 read snapshot (B1.2)

**`ext_read_runs`** (X2)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| connection_id | uuid | no | composite FK → `integration_connections` |
| purpose | text | no | `CHECK IN ('wf02_match','wf04_stock','wf05_report','wf06_replenish','wf07_custom')` |
| status | text | no | `CHECK IN ('running','succeeded','partial','failed')`, default `'running'` |
| partial_reason | text | yes | `CHECK IN ('row_ceiling','page_limit')`; `CHECK ((status = 'partial') = (partial_reason IS NOT NULL))` |
| pages | integer | no | default 0, `>= 0` |
| rows | integer | no | default 0, `>= 0` |
| error_class | text | yes | same list as `workflow_executions.error_class`; `CHECK ((status = 'failed') = (error_class IS NOT NULL))` |
| started_at | timestamptz | no | default `now()` |
| finished_at | timestamptz | yes | `CHECK ((status = 'running') = (finished_at IS NULL))` |

**`ext_records`** (X1; one row per fetched record or line; never updated)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| connection_id | uuid | no | composite FK |
| read_run_id | uuid | no | composite FK → `ext_read_runs` ON DELETE CASCADE |
| kind | text | no | `CHECK IN ('purchase_order','purchase_order_line','receipt','receipt_line','bill','bill_line','product_policy','stock_quant','stock_location','stock_move_line','inventory_location','delivery','sales_order','invoice','fulfillment','orderpoint','supplier_price')` (full list created by B1.2) |
| ext_id | text(1..100) | no | provider id (line id for `*_line`) |
| parent_ext_id | text(1..100) | yes | header id of a line |
| item_ref, uom_ref, location_ref, currency_ref | text(1..100) | yes | source ids, mapped later through `integration_mappings` |
| qty | numeric | yes | exact |
| amount | numeric | yes | exact, in `currency_ref` |
| doc_date | date | yes | plain date, never shifted (NetSuite `Date`) |
| event_at | timestamptz | yes | UTC |
| state | text(1..64) | yes | provider state as read |
| source_modified_at | timestamptz | yes | Odoo `write_date` / NetSuite `lastModifiedDate` |
| fetched_at | timestamptz | no | |
| raw | jsonb | no | object, `octet_length(raw::text) <= 65536` |

Index: `UNIQUE (workspace_id, connection_id, kind, ext_id, read_run_id)`; `(read_run_id, kind)`.

X7 fetch helpers (Rust, B1.2): `integrations::fetch::odoo_pages(client, model, domain, fields, order, limit)` (`search_read` `limit/offset/order`, O1) and `integrations::fetch::netsuite_pages(client, path, q, limit)` (`limit/offset`, offset divisible by limit, `hasMore`, R2-N8). Ceiling 100,000 rows → run `partial` + `row_ceiling`. 429 → `ErrorClass::Transient`.

### 11.3 X3 review cases (B2.1) and X4 incidents + expectations (B1.1)

**`review_cases`** (X3)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| workflow_key | text | no | `CHECK IN ('wf02_match')` (B2.2 adds `'wf04_stock'`) |
| pair_id | uuid | no | composite FK → `integration_pairs` |
| subject_ref | text(1..300) | no | WF-02: the PO line ref `odoo:<connection_id>:purchase.order.line:<id>` (`netsuite:<connection_id>:purchaseOrder.item:<po id>:<line>` after L7); WF-04: `stock:<scope_id>:<product mapping_id>` |
| reason | text | no | `CHECK IN ('over_billed','over_received','price_variance','uom_differs','currency_mismatch','unmapped_product','stale_snapshot','stock_discrepancy')` |
| status | text | no | `CHECK IN ('open','awaiting_correction','accepted','resolved_verified','superseded')`, default `'open'` |
| read_run_id | uuid | no | composite FK → `ext_read_runs`; the snapshot the case was opened or last refreshed from |
| evidence | jsonb | no | object ≤ 64 KiB; `{"record_ids":[uuid…], "summary": "readable"}` citing `ext_records` ids |
| created_at, updated_at | timestamptz | no | |

Index: `UNIQUE (workspace_id, workflow_key, subject_ref, reason) WHERE status IN ('open','awaiting_correction')` (`review_cases_one_open`).

**`review_case_decisions`** (AO)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| case_id | uuid | no | composite FK → `review_cases` |
| decision | text | no | `CHECK IN ('accept','request_correction','resolve_verified','supersede')` |
| actor_kind | text | no | `CHECK IN ('user','system')`; `CHECK ((actor_kind = 'user') = (decision IN ('accept','request_correction')))` |
| actor_user_id | uuid | yes | `REFERENCES users (id)`; `CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))` |
| reason | text(1..2000) | yes | `CHECK (actor_kind = 'system' OR reason IS NOT NULL)` |
| snapshot_seen | uuid | yes | the `read_run_id` the user saw; required for user decisions |
| created_at | timestamptz | no | `clock_timestamp()` |

**Case state machine**
| from | to | trigger | writer |
|---|---|---|---|
| (new) | `open` | result `mismatch`/`unverifiable` (WF-02) or `discrepancy` (WF-04) | matcher / stock check, idempotent via `review_cases_one_open` |
| `open` | `accepted` | `accept` + reason (WF-04 UI label "Explained") | decide route |
| `open` | `awaiting_correction` | `request_correction` + reason | decide route |
| `open`, `awaiting_correction` | `resolved_verified` | a later run shows `matched`/`within_tolerance` (WF-02) or `equal`/`within_threshold` (WF-04) | matcher / stock check (system decision `resolve_verified`) |
| `open`, `awaiting_correction` | `superseded` | PO cancelled or line gone (WF-02); scope or mapping removed (WF-04) | same (system decision `supersede`) |

Terminal: `accepted`, `resolved_verified`, `superseded`. Decide is a CAS on `status` and on the source: if the source record's `source_modified_at` read now (Odoo `write_date`, NetSuite `lastModifiedDate`) is newer than the one in `snapshot_seen`, or a newer read run exists for the case → 409 `snapshot_stale`.

**`workflow_incidents` widening** (X4, B1.1)
- `kind` += `exchange_late`, `exchange_failed`, `auth_failure`, `permission_denied`, `rate_limited`, `mapping_invalid` (B4 adds `waiting_data`).
- New columns: `category text NULL CHECK IN ('transient','authorization','data','conflict','missing','unknown_outcome')` (NULL only on WF-01 kinds); `diagnosis jsonb NULL` (object ≤ 16 KiB, shape below); `verified_at timestamptz NULL`; `resolution_kind text NULL CHECK IN ('reconciled','manual_closed')`.
- New CHECKs: `CHECK (resolution_kind <> 'reconciled' OR verified_at IS NOT NULL)`; `CHECK ((status = 'resolved') = (resolved_at IS NOT NULL AND resolution_kind IS NOT NULL))`; `CHECK (resolution_kind IS DISTINCT FROM 'manual_closed' OR (resolved_by IS NOT NULL AND note IS NOT NULL))`.
- Dropped and replaced: the WF-01 CHECKs `(status = 'resolved') = (resolved_by IS NOT NULL AND resolved_at IS NOT NULL)` and `status = 'open' OR note IS NOT NULL` (a system auto-verify resolves with no user and no note). The up migration backfills `resolution_kind = 'manual_closed'` on existing resolved rows; the down migration restores the WF-01 CHECKs.
- `diagnosis`: `{"code": "<ErrorClass or provider code>", "sentence": "readable", "records": [{"subject_ref","target_id"|null}], "last_attempt": {"attempt_no","outcome","http_status"}|null}`. The UI shows `sentence`, never `code`.

**`exchange_expectations`** (X4)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| workflow_key | text | no | `CHECK IN ('wf01_purchase')` (B3 adds `wf06_replenish`, B4 adds `wf07_custom`) |
| correlation_id | text | no | `^vz-[0-9a-f]{32}$`; target connection comes from `external_links` by this id |
| execution_id | uuid | yes | composite FK → `workflow_executions` |
| source_ref | text(1..300) | no | |
| target_kind | text(1..100) | no | e.g. `purchaseOrder`, `purchase.order`, `invoice` |
| target_external_id | text(1..100) | no | |
| due_at | timestamptz | no | write intent + 15 min (D-32) |
| status | text | no | `CHECK IN ('pending','met','late','failed','closed')`, default `'pending'` |
| checks | smallint | no | default 0, `CHECK (checks BETWEEN 0 AND 10)` |
| last_checked_at | timestamptz | yes | |
| incident_id | uuid | yes | composite FK → `workflow_incidents` |
| created_at, updated_at | timestamptz | no | |

Index: `UNIQUE (workspace_id, correlation_id)`; `(status, due_at) WHERE status IN ('pending','late')`.

**Expectation state machine**
| from | to | trigger | writer |
|---|---|---|---|
| (new) | `pending` | write intent of a Flow-run write (WF-01, WF-06, WF-07) | the writer, same tx as `begin_attempt` |
| `pending` | `met` | read-back matches | `wf03_expect_sweep` or the writer's verify |
| `pending` | `late` | `due_at` passed, target missing; opens incident `exchange_late` | `wf03_expect_sweep` |
| `late` | `met` | target appears and matches; incident → `resolved` with `resolution_kind='reconciled'`, `verified_at` set | `wf03_expect_sweep`, `…/incidents/{incident}/verify` |
| `pending`, `late` | `failed` | permanent class (`Validation`, `Permission`, `Auth`, `Conflict`); incident `exchange_failed` (or `auth_failure`/`permission_denied`) | sweep / writer |
| `failed`, `late` | `closed` | linked incident resolved | incident resolve route |

`checks` reaching 10 stops calls; the row stays `late` for humans.

**Category map** (`src/workflows/recovery/category.rs`, one test per class): `Transient`→`transient` (HTTP 429 → kind `rate_limited`); `Auth`, `Permission`→`authorization`; `Validation`→`data` (kind `mapping_invalid` when the provider detail names a mapped field); `Conflict`→`conflict`; `NotFound`→`missing`; `Ambiguous`, `Unknown`→`unknown_outcome`.

### 11.4 X5 capability keys (added to `capability.rs` by the child that first uses them)

| const | key | provider lists | child |
|---|---|---|---|
| `INVOICE_LIST` | `invoice.list` | NetSuite | B1.2 |
| `DELIVERY_LIST` | `delivery.list` | Odoo | B1.2 |
| `SALES_ORDER_FIND` | `sales_order.find_by_external_id` | NetSuite | B1.2 |
| `PURCHASE_MATCH_READ` | `purchase_match.read` | Odoo (NetSuite after L7) | B2.1 |
| `STOCK_ON_HAND_READ` | `stock_on_hand.read` | Odoo, NetSuite | B2.2 |
| `STOCK_MOVEMENT_READ` | `stock_movement.read` | Odoo | B2.2 |
| `ORDERPOINT_LIST` | `orderpoint.list` | Odoo | B3 |
| `SUPPLIER_PRICE_READ` | `supplier_price.read` | Odoo | B3 |
| `REPLENISH_REQUEST` | `replenish.request` | Odoo (write, `action_replenish`) | B3 |
| `SALES_ORDER_LIST_CHANGED` | `sales_order.list_changed` | NetSuite | B4 |
| `FULFILLMENT_READ` | `fulfillment.read` | Odoo, NetSuite | B4 |
| `INVOICE_FROM_SALES_ORDER` | `invoice.create_from_sales_order` | NetSuite (write, transform) | B4 |

Probe checks: Odoo module presence via `fields_get` (R2-O9); NetSuite via `metadata-catalog?select=` (R2-N10). A failed key appears as `probe_failed:<key>` (§3.6).

### 11.5 Jobs (`src/jobs/kinds.rs`; each child adds only its own kinds)

Scheduled kinds are system jobs (no workspace) in `SCHEDULES`, like `wf01_tick`. Where the row says "fan-out", the system run enqueues one workspace job **of the same kind** per active pair, with payload `{"pair_id"}`; the per-pair job carries the keys shown.

| kind const | name | child | enqueue | keys |
|---|---|---|---|---|
| `WF03_EXPECT_SWEEP` | `wf03_expect_sweep` | B1.1 | `SCHEDULES` `Every::Minutes(5)` | system job |
| `WF03_HEALTH` | `wf03_health` | B1.1 | `SCHEDULES` `Every::Minutes(15)` | system job; one probe per non-`disconnected` connection |
| `WF05_REPORT_RUN` | `wf05_report_run` | B1.2 | `POST …/reports/cross/runs` and scheduled alerts | `dedupe_key = <report_run_id>` |
| `WF02_MATCH_SCAN` | `wf02_match_scan` | B2.1 | `SCHEDULES` `Every::Minutes(60)`, fan-out | `dedupe_key = "wf02_match_scan:<pair>"`, `concurrency_key = "wf02:<pair>"` |
| `WF02_MATCH_PO` | `wf02_match_po` | B2.1 | scan, `…/matches/{po_ref}/rerun` | `dedupe_key = "<pair>:<po_ref>"`, `concurrency_key = "wf02:<pair>"` |
| `WF04_STOCK_CHECK` | `wf04_stock_check` | B2.2 | `SCHEDULES` `Every::Minutes(60)`, fan-out; `POST …/stock-checks` | `dedupe_key = "wf04_stock_check:<pair>"`, `concurrency_key = "netsuite:<netsuite connection_id>"` |
| `WF06_SCAN` | `wf06_scan` | B3 | `SCHEDULES` `Every::Minutes(60)`, fan-out; `POST …/replenishment/scan` | `dedupe_key = "wf06_scan:<pair>"`, `concurrency_key = "wf06:<pair>"` |
| `WF06_EXECUTE` | `wf06_execute` | B3 | `dispatch::enqueue_execution` arm `"wf06_replenish"` | `dedupe_key = <execution_id>`, `max_attempts = 5` |
| `WF06_RECONCILE` | `wf06_reconcile` | B3 | execute, on `awaiting_reconciliation` | `dedupe_key = "wf06_reconcile:<execution_id>"`, `max_attempts = 10` |
| `WF07_TICK` | `wf07_tick` | B4 | `SCHEDULES` `Every::Minutes(1)` | system job; advances each run one step (CAS on `current_step`) |

`SCHEDULES` grows by 2 (B1.1), 1 (B2.1), 1 (B2.2), 1 (B3), 1 (B4). Execution of WF-07 action steps reuses `dispatch::enqueue_execution` with arm `"wf07_custom"` (B4) and kinds `wf07_execute` / `wf07_reconcile` with the same keys as WF-06 (`WF07_EXECUTE`, `WF07_RECONCILE`).

Cursor streams (§2.1 `integration_poll_cursors`): `wf02_match:<pair_id>`, `wf07_custom:<definition_id>`.

### 11.6 WF-02 PO, receipt and bill matching (B2.1)

Pair config adds `triad_owner: "odoo" | "netsuite"` (default `"odoo"`, D-21).

**`match_tolerances`**: `pair_id` (FK), `currency_ref text(1..64)`, `qty_pct numeric NOT NULL DEFAULT 0 CHECK (>= 0)`, `qty_abs numeric NOT NULL DEFAULT 0 CHECK (>= 0)`, `amount_pct numeric NOT NULL DEFAULT 0 CHECK (>= 0)`, `amount_abs_minor bigint NOT NULL DEFAULT 0 CHECK (>= 0)`. `UNIQUE (workspace_id, pair_id, currency_ref)`. No row = 0 (exact, D-24).

**`match_runs`**: `pair_id` (FK), `po_ref text(1..300)` (`odoo:<connection_id>:purchase.order:<id>`), `read_run_id uuid NULL` (FK → `ext_read_runs`), `inputs_hash text CHECK (~ '^[0-9a-f]{64}$')`, `status CHECK IN ('running','done','failed')`, `error_class` (same list as executions, NULL unless `failed`). `UNIQUE (workspace_id, pair_id, po_ref, inputs_hash)`.

**`match_lines`** (filled by one `INSERT … SELECT`; never updated)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| run_id | uuid | no | composite FK → `match_runs` ON DELETE CASCADE |
| po_line_ref | text(1..300) | no | `UNIQUE (run_id, po_line_ref)` |
| ordered_qty, received_qty, billed_qty | numeric | no | in the PO line UoM (R2-O2) |
| ordered_amount, billed_amount | numeric | yes | untaxed; NULL until F1 |
| currency_ref | text(1..64) | no | PO currency |
| control | text | no | `CHECK IN ('ordered','received')` (`purchase_method` `purchase` → `ordered`, `receive` → `received`) |
| result | text | no | `CHECK IN ('matched','within_tolerance','awaiting_receipt','awaiting_bill','mismatch','unverifiable')` |
| reasons | text[] | no | default `'{}'`; values from the `review_cases.reason` list; `CHECK ((result IN ('mismatch','unverifiable')) = (cardinality(reasons) > 0))` |
| draft_billed | boolean | no | default false; true when a draft bill counted (D-25 label "draft") |
| record_ids | uuid[] | no | the `ext_records` ids used |

**Routes**
| method + path | permission | body | success | errors | operation_id |
|---|---|---|---|---|---|
| GET `/workspace/{workspace}/workflows/matches?pair=&result=&page=&page_size=` | view | — | list of `MatchSummary` | 422 unknown result | `workflows_matches_list` |
| GET `/workspace/{workspace}/workflows/matches/{po_ref}` | view | — | `{"data":MatchDetail}` (latest run) | 404 | `workflows_matches_get` |
| POST `/workspace/{workspace}/workflows/matches/{po_ref}/rerun` | view | — | 202 `{"message":"ok"}` | 404 | `workflows_matches_rerun` |
| GET `/workspace/{workspace}/review-cases?workflow_key=&status=&page=&page_size=` | view | — | list of `ReviewCase` | 422 unknown key | `review_cases_list` |
| POST `/workspace/{workspace}/review-cases/{case}/decide` | reconcile | `{"decision":"accept\|request_correction","reason","snapshot_seen":"uuid"}` + `Idempotency-Key` | `{"data":ReviewCase}` | 409 `snapshot_stale`, 409 `review_case_already_decided`, 422 reason | `review_cases_decide` |
| GET `/workspace/{workspace}/integrations/pairs/{pair}/tolerances` | view | — | `{"data":[Tolerance]}` | 404 | `integrations_tolerances_get` |
| POST `/workspace/{workspace}/integrations/pairs/{pair}/tolerances` | manage | `{"tolerances":[Tolerance]}` (full replace) | `{"message":"ok"}` | 404, 422 | `integrations_tolerances_update` |

`{po_ref}` is the percent-encoded `po_ref` text; an unknown or other-workspace ref → 404.

```jsonc
// MatchSummary
{ "po_ref", "pair_id", "po_name": "P00017|null", "currency_ref", "worst_result": "matched|…|unverifiable",
  "open_cases": 0, "as_of": "rfc3339 (read run fetched_at)", "run_id" }
// MatchDetail = MatchSummary + { "lines": [ { "po_line_ref", "product_label", "uom_label", "control": "ordered|received",
//   "ordered_qty": "10.00000000", "received_qty", "billed_qty", "ordered_amount": "…|null", "billed_amount": "…|null",
//   "draft_billed": false, "result", "reasons": ["over_billed"], "sources": [ { "record_id", "kind", "ext_id", "label" } ] } ],
//   "payment_state": "native value|null" }
// Tolerance
{ "currency_ref": "EUR", "qty_pct": "2", "qty_abs": "0", "amount_pct": "0", "amount_abs_minor": 0 }
// ReviewCase
{ "id", "workflow_key": "wf02_match|wf04_stock", "pair_id", "subject_ref", "reason", "status", "read_run_id", "as_of",
  "evidence": { "record_ids": [], "summary": "readable" },
  "decisions": [ { "decision", "actor_kind", "actor": { "user_id", "name" } | null, "reason", "created_at" } ],
  "created_at", "updated_at" }
```

### 11.7 WF-03 recovery routes (B1.1)

| method + path | permission | body | success | errors | operation_id |
|---|---|---|---|---|---|
| GET `/workspace/{workspace}/workflows/incidents?status=&kind=&category=&page=&page_size=` (extended) | view | — | list of `Incident` | 422 unknown category | `workflows_incidents_list` |
| GET `/workspace/{workspace}/workflows/incidents/{incident}` | view | — | `{"data":IncidentDetail}` | 404 | `workflows_incidents_get` |
| POST `/workspace/{workspace}/workflows/incidents/{incident}/verify` | reconcile | — + `Idempotency-Key` | `{"data":IncidentDetail}` | 409 `incident_already_resolved`, 422 `incident_not_verifiable` | `workflows_incidents_verify` |
| POST `/workspace/{workspace}/workflows/incidents/{incident}/resolve` (extended) | reconcile, not the requester | `{"note","resolution_kind":"reconciled\|manual_closed"}` (default `manual_closed`) + `Idempotency-Key` | `{"data":Incident}` | + 409 `incident_not_verified` | `workflows_incidents_resolve` |

Retry stays `POST …/executions/{execution}/retry`.

```jsonc
// Incident (extends §5.5)
{ …§5.5…, "category": "transient|authorization|data|conflict|missing|unknown_outcome|null",
  "diagnosis": { "sentence", "records": [], "last_attempt": {…}|null } | null,
  "verified_at": "rfc3339|null", "resolution_kind": "reconciled|manual_closed|null" }
// IncidentDetail = Incident + { "expectation": { "status","due_at","checks","last_checked_at","target_kind","target_external_id" } | null,
//   "timeline": [ { "kind","actor_kind","created_at","summary" } ] }   // from workflow_events
```

### 11.8 WF-04 stock (B2.2)

Pair config adds `stock_freshness_minutes: int 1..1440` (default 60) and `stock_threshold: "decimal string ≥ 0"` (default `"0"`) (D-43).

**`stock_location_scopes`**: `pair_id` (FK), `odoo_location_ids text[] NOT NULL CHECK (cardinality >= 1)`, `netsuite_location_id text(1..100)`, `expected_equal boolean NOT NULL DEFAULT false` (D-42), `label text(1..100)`. `UNIQUE (workspace_id, pair_id, netsuite_location_id)`.

**`stock_checks`**: `pair_id` (FK), `odoo_read_run_id uuid NULL`, `netsuite_read_run_id uuid NULL` (FKs → `ext_read_runs`), `definition text NOT NULL DEFAULT 'on_hand' CHECK (definition IN ('on_hand'))`, `freshness_minutes int NOT NULL`, `status CHECK IN ('running','done','failed')`, `error_class` (executions list, NULL unless `failed`).

**`stock_check_lines`** (one `INSERT … SELECT`; never updated)
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| check_id | uuid | no | composite FK → `stock_checks` ON DELETE CASCADE |
| item_map_id | uuid | no | composite FK → `integration_mappings` (entity `product`) |
| scope_id | uuid | no | composite FK → `stock_location_scopes` |
| odoo_qty, netsuite_qty, diff | numeric | yes | `diff = odoo_qty - netsuite_qty` in SQL |
| odoo_fetched_at, netsuite_fetched_at | timestamptz | yes | |
| result | text | no | `CHECK IN ('equal','within_threshold','discrepancy','stale','unit_unmapped','not_expected_equal')` |
| reasons | text[] | no | default `'{}'`; `CHECK (reasons <@ ARRAY['item_not_found','permission_denied'])` |

`UNIQUE (check_id, item_map_id, scope_id)`. Case reason `stock_discrepancy`; evidence = quant record ids, NetSuite location record id, last 20 done move-line record ids.

| method + path | permission | body | success | errors | operation_id |
|---|---|---|---|---|---|
| GET `/workspace/{workspace}/integrations/pairs/{pair}/stock-scopes` | view | — | `{"data":[StockScope]}` | 404 | `integrations_stock_scopes_get` |
| POST `/workspace/{workspace}/integrations/pairs/{pair}/stock-scopes` | manage | `{"scopes":[StockScopeInput]}` upsert by `netsuite_location_id`; absent ones deleted | `{"message":"ok"}` | 404, 422, 409 `scope_in_use` | `integrations_stock_scopes_update` |
| POST `/workspace/{workspace}/workflows/stock-checks` | view | `{"pair_id"}` + `Idempotency-Key` | 202 `{"data":{"stock_check_id"}}` | 404 pair | `workflows_stock_checks_create` |
| GET `/workspace/{workspace}/workflows/stock-checks?pair=&page=&page_size=` | view | — | list of `StockCheck` (no lines) | | `workflows_stock_checks_list` |
| GET `/workspace/{workspace}/workflows/stock-checks/{stock_check}?result=` | view | — | `{"data":StockCheck}` with `lines` | 404, 422 unknown result | `workflows_stock_checks_get` |

Review cases: the WF-02 routes with `workflow_key=wf04_stock`.

```jsonc
// StockScope / StockScopeInput (input has no id)
{ "id", "odoo_location_ids": ["8","12"], "netsuite_location_id": "3", "expected_equal": false, "label": "Main WH" }
// StockCheck
{ "id", "pair_id", "definition": "on_hand", "status": "running|done|failed", "error_category": "…|null",
  "freshness_minutes": 60, "counts": { "equal": 0, "within_threshold": 0, "discrepancy": 0, "stale": 0, "unit_unmapped": 0, "not_expected_equal": 0 },
  "created_at",
  "lines": [ { "item_label", "scope_label", "odoo_qty": "12.00000000|null", "netsuite_qty", "diff", "odoo_fetched_at", "netsuite_fetched_at",
               "result", "reasons": [], "case_id": "uuid|null" } ] }   // detail only
```
`counts` come from a SQL `GROUP BY result`.

### 11.9 WF-05 cross-ERP reports (B1.2)

**`report_join_rules`**: `pair_id` (FK), `rule_key text CHECK (rule_key IN ('so_external_id_template'))`, `template text(1..200) CHECK (template LIKE '%{source_id}%')` (e.g. `odoo-so-{source_id}`, D-51). `UNIQUE (workspace_id, pair_id, rule_key)`.

**`report_definitions`**: `pair_id` (FK), `template_key text CHECK IN ('fulfilled_without_invoice','rfq_vs_po')`, `params jsonb NOT NULL` (object ≤ 4 KiB, validated in Rust per template), `saved boolean NOT NULL DEFAULT false`, `created_by uuid REFERENCES users (id)`.

**`report_runs`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| definition_id | uuid | yes | composite FK → `report_definitions` |
| pair_id | uuid | no | composite FK |
| template_key | text | no | same CHECK as definitions |
| params | jsonb | no | object ≤ 4 KiB |
| tz | text(1..64) | no | workspace time zone (D-53) |
| date_from, date_to | date | no | `CHECK (date_from <= date_to)` |
| status | text | no | `CHECK IN ('queued','running','succeeded','partial','failed')`, default `'queued'` |
| odoo_read_run_id, netsuite_read_run_id | uuid | yes | composite FKs → `ext_read_runs` |
| as_of | timestamptz | yes | oldest `fetched_at` of the two read runs |
| created_at, updated_at | timestamptz | no | |

**`report_rows`** (never updated): `run_id` (FK, CASCADE), `left_record_id uuid NOT NULL` (FK → `ext_records`), `right_record_ids uuid[] NOT NULL DEFAULT '{}'`, `match_state text CHECK IN ('matched','unmatched','ambiguous')`, `currency_ref text(1..64) NULL`, `amount numeric NULL`. `CHECK ((match_state = 'unmatched') = (cardinality(right_record_ids) = 0))`, `CHECK ((match_state = 'ambiguous') = (cardinality(right_record_ids) > 1))`. Totals are `SELECT currency_ref, match_state, sum(amount), count(*) … GROUP BY currency_ref, match_state` at read time; no stored totals.

`scheduled_reports.report` += `cross_erp_exceptions` (D-54; mails only the member's own login).

Template params (both templates): `{"date_from":"yyyy-mm-dd","date_to":"yyyy-mm-dd"}`; any other key → 422 field error (`errors.params`).

| method + path | permission | body | success | errors | operation_id |
|---|---|---|---|---|---|
| GET `/workspace/{workspace}/reports/cross/templates` | view | — | `{"data":[ReportTemplate]}` | | `reports_cross_templates_list` |
| POST `/workspace/{workspace}/reports/cross/runs` | view | `{"template_key","pair_id","date_from","date_to"}` + `Idempotency-Key` | 202 `{"data":{"report_run_id"}}` | 404 pair, 422 fields | `reports_cross_runs_create` |
| GET `/workspace/{workspace}/reports/cross/runs/{run}?match_state=&page=&page_size=` | view | — | `{"data":ReportRun}` | 404 | `reports_cross_runs_get` |
| POST `/workspace/{workspace}/reports/cross/definitions` | view | `{"template_key","pair_id","params","saved"}` | 201 `{"data":{"report_definition_id"}}` | 422 | `reports_cross_definitions_create` |
| GET `/workspace/{workspace}/reports/cross/definitions` | view | — | list of `ReportDefinition` | | `reports_cross_definitions_list` |

```jsonc
// ReportTemplate
{ "key": "fulfilled_without_invoice|rfq_vs_po", "title": "readable", "params": ["date_from","date_to"] }
// ReportRun
{ "id", "template_key", "pair_id", "status": "queued|running|succeeded|partial|failed", "partial_reason": "row_ceiling|page_limit|null",
  "error_category": "…|null", "tz": "Europe/Bucharest", "date_from", "date_to", "as_of": "rfc3339|null",
  "totals": [ { "currency_ref": "EUR", "match_state": "unmatched", "amount": "123.45", "count": 3 } ],
  "counts": { "matched": 0, "unmatched": 0, "ambiguous": 0 },
  "rows": [ { "left": { "record_id", "kind", "ext_id", "label", "doc_date", "event_at" },
              "right": [ { "record_id", "kind", "ext_id", "label" } ], "match_state", "currency_ref", "amount" } ] }
// ReportDefinition
{ "id", "template_key", "pair_id", "params": {}, "saved": true, "created_by": { "user_id","name" }, "created_at" }
```

### 11.10 WF-06 replenishment (B3)

**`replenish_proposals`**
| column | type | null | rule |
|---|---|---|---|
| id, workspace_id | uuid | no | |
| pair_id | uuid | no | composite FK |
| orderpoint_ref, product_ref, location_ref | text(1..300) | no | source refs `odoo:<connection_id>:<model>:<id>` |
| supplier_ref | text(1..300) | yes | |
| qty | numeric | no | = Odoo `qty_to_order`, `CHECK (qty > 0)` |
| uom_ref | text(1..300) | no | |
| unit_price | numeric | yes | supplierinfo price, unrounded |
| currency_ref | text(1..64) | yes | |
| est_cost | numeric | yes | `unit_price × qty` rounded to the currency's decimals, in SQL; label "estimate" |
| rationale | jsonb | no | `{"min","max","forecast","in_progress","qty_to_order","read_run_ids":[uuid]}` (decimal strings) |
| status | text | no | `CHECK IN ('draft','blocked','pending_approval','approved','rejected','executing','done','superseded')` |
| blocked_reasons | text[] | no | default `'{}'`; values `missing_supplier`, `rfq_linked_to_netsuite_po`, `netsuite_on_order_unacknowledged`, `unmapped`; `CHECK ((status = 'blocked') = (cardinality(blocked_reasons) > 0))` |
| on_order_acknowledged_by | uuid | yes | `REFERENCES users (id)` |
| approval_request_id | uuid | yes | composite FK |
| execution_id | uuid | yes | composite FK |
| result_rfq_ref | text(1..300) | yes | `CHECK (status <> 'done' OR result_rfq_ref IS NOT NULL)` |
| created_at, updated_at | timestamptz | no | |

Index: `UNIQUE (workspace_id, pair_id, orderpoint_ref) WHERE status IN ('draft','blocked','pending_approval','approved','executing')` (`replenish_proposals_one_open`).

**State machine**
| from | to | trigger | writer |
|---|---|---|---|
| (new) | `draft` / `blocked` | `wf06_scan` (auto orderpoints skipped, D-62) | scan |
| `blocked` | `draft` | reasons cleared (rescan or acknowledge) | scan / acknowledge route |
| `draft` | `pending_approval` | submit (creates the approval request) | submit route |
| `pending_approval` | `approved` / `rejected` | approval decided (existing approve/reject/return; `return` → `rejected`) | approvals decide, same tx |
| `approved` | `executing` | execution row created, `wf06_execute` enqueued | approvals decide |
| `executing` | `done` | draft RFQ read back (`finish_succeeded`) | `wf06_execute` / `wf06_reconcile` |
| any non-terminal | `superseded` | next scan `qty_to_order = 0` | scan |

Terminal: `rejected`, `done`, `superseded`. Approval request for a proposal: `workflow_key = 'wf06_replenish'`, `subject_ref = orderpoint_ref`, `required_permission = 'integration.approve'` (D-64, no threshold policy), `amount_minor` = `est_cost` in minor units (SQL; 0 when `est_cost` is NULL), snapshot `{"v":1,"proposal":{…row…}}`. Execution: `target_ref = odoo:<connection_id>:purchase.order:<rfq id>`; expectation `target_kind = 'stock.warehouse.orderpoint'`, `target_external_id = <orderpoint id>`.

| method + path | permission | body | success | errors | operation_id |
|---|---|---|---|---|---|
| GET `/workspace/{workspace}/workflows/replenishment?pair=&status=&page=&page_size=` | view | — | list of `ReplenishProposal` | 422 unknown status | `workflows_replenishment_list` |
| POST `/workspace/{workspace}/workflows/replenishment/scan` | approve | `{"pair_id"}` | 202 `{"message":"ok"}` | 404 pair | `workflows_replenishment_scan` |
| POST `/workspace/{workspace}/workflows/replenishment/{proposal}/submit` | approve | — + `Idempotency-Key` | 201 `{"data":{"approval_request_id"}}` | 409 `proposal_state_changed` | `workflows_replenishment_submit` |
| POST `/workspace/{workspace}/workflows/replenishment/{proposal}/acknowledge` | approve | `{"reason"}` + `Idempotency-Key` | `{"data":ReplenishProposal}` | 409 `proposal_state_changed` | `workflows_replenishment_acknowledge` |

```jsonc
// ReplenishProposal
{ "id", "pair_id", "status", "blocked_reasons": ["missing_supplier"], "product_label", "location_label", "supplier_label": "…|null",
  "qty": "5.00000000", "uom_label", "unit_price": "1.335|null", "currency_ref": "EUR|null", "est_cost": "4.01|null",
  "rationale": { "min","max","forecast","in_progress","qty_to_order" }, "netsuite_on_order": "0|null", "as_of",
  "approval_request_id": "uuid|null", "execution": ExecutionSummary | null, "result_rfq_ref": "…|null", "created_at", "updated_at" }
```

### 11.11 WF-07 definitions and runs (B4)

**`workflow_definitions`**: `key text CHECK (key ~ '^[a-z][a-z0-9_]{0,63}$')`, `title text(1..200)`, `status CHECK IN ('draft','published','retired')` default `'draft'`, `created_by uuid REFERENCES users (id)`. `UNIQUE (workspace_id, key)`.

**`workflow_definition_versions`**: `definition_id` (FK), `version int CHECK (version >= 1)`, `spec jsonb` (object ≤ 65536), `spec_hash text CHECK (~ '^[0-9a-f]{64}$')`, `validation jsonb` (ValidationReport), `created_by uuid REFERENCES users (id)`, `published_by uuid NULL REFERENCES users (id)`, `published_at timestamptz NULL`. `UNIQUE (definition_id, version)`; `CHECK ((published_by IS NULL) = (published_at IS NULL))`; `CHECK (published_by IS NULL OR published_by <> created_by)` (D-74). Trigger `workflow_definition_versions_frozen`: UPDATE when `OLD.published_at IS NOT NULL`, or DELETE, raises `restrict_violation` (K5 style).

**`workflow_runs`**: `version_id` (FK), `trigger_ref text(1..300)` (`netsuite:<connection_id>:salesOrder:<id>`), `correlation_id` (`^vz-[0-9a-f]{32}$`, `UNIQUE (workspace_id, correlation_id)`), `status CHECK IN ('running','waiting_approval','waiting_data','waiting_handoff','failed','succeeded','cancelled')`, `current_step int NOT NULL DEFAULT 0 CHECK (>= 0)`.

**`workflow_run_steps`**: `run_id` (FK), `step_no int CHECK (>= 0)`, `step_key text(1..64)`, `capability_key text(1..100) NULL`, `status CHECK IN ('pending','running','waiting','done','failed','skipped','handed_off')`, `input jsonb`, `output jsonb NULL`, `due_at timestamptz NULL` (waits), `approval_request_id uuid NULL`, `execution_id uuid NULL` (composite FKs). `UNIQUE (run_id, step_no)`.

**Spec JSON v1** (what Verzo drafts and the validator reads)
```jsonc
{ "v": 1, "title": "Wholesale order",
  "steps": [ { "key": "s1", "kind": "trigger|condition|approval|action|wait_until|manual_handoff",
               "library": "sales_order_created|approval|check_stock|request_purchasing|wait_fulfillment|invoice_after_fulfillment|manual_handoff",
               "params": { … per library step … },
               "condition": { "left": "s2.output.short_qty", "op": "eq|ne|gt|gte|lt|lte|exists", "right": "0" } | null,
               "instructions": "for manual_handoff|null" } ] }
```
Condition comparisons of numbers run in SQL `numeric`; no expressions, no code.

**Step library v1**
| library key | kind | capability key | side_effect | gate |
|---|---|---|---|---|
| `sales_order_created` | trigger | `sales_order.list_changed` | no | L10 |
| `approval` | approval | — (Flow approval) | no | — |
| `check_stock` | action (read) | `stock_on_hand.read` | no | L11 |
| `request_purchasing` | action | `replenish.request` (via a WF-06 proposal) | yes | L12 |
| `wait_fulfillment` | wait_until | `fulfillment.read` | no | L11 / L13 |
| `invoice_after_fulfillment` | action | `invoice.create_from_sales_order` | yes | **L14** |
| `manual_handoff` | manual_handoff | — | no | — |

**Definition state machine:** `draft` → `published` (first version published) → `retired` (route). A version is immutable once `published_at` is set. A run pins `version_id`.

**Run state machine**
| from | to | trigger |
|---|---|---|
| (new) | `running` | trigger step fires (`wf07_tick`) |
| `running` | `waiting_approval` / `waiting_data` / `waiting_handoff` | approval step, wait step, handoff step |
| `waiting_approval` | `running` | approval approved; rejected/returned → `failed` |
| `waiting_data` | `running` | wait read satisfied; deadline passed opens incident `waiting_data` (run stays `waiting_data`) |
| `waiting_handoff`, `waiting_data` | `running` | `POST …/runs/{run}/resume` |
| `running` | `succeeded` / `failed` | last step done / step failed |
| any non-terminal | `cancelled` | `POST …/runs/{run}/cancel` (never undoes side effects; event recorded) |

Terminal: `succeeded`, `failed`, `cancelled`. Step: `pending → running → done | failed | waiting | handed_off`; `waiting → done | failed`; `pending → skipped` (condition false).

| method + path | permission | body | success | errors | operation_id |
|---|---|---|---|---|---|
| POST `/workspace/{workspace}/workflows/definitions/validate` | manage | `{"spec"}` | `{"data":ValidationReport}` | 422 not an object | `workflows_definitions_validate` |
| POST `/workspace/{workspace}/workflows/definitions` | manage | `{"key","title","steps"}` (new key → definition + v1; existing key → next draft version) | 201 `{"data":{"definition_id","version"}}` | 422 | `workflows_definitions_create` |
| GET `/workspace/{workspace}/workflows/definitions` | view | — | list of `WorkflowDefinition` | | `workflows_definitions_list` |
| GET `/workspace/{workspace}/workflows/definitions/{definition}` | view | — | `{"data":WorkflowDefinition}` with `versions` | 404 | `workflows_definitions_get` |
| POST `/workspace/{workspace}/workflows/definitions/{definition}/versions/{version}/test` | manage | — | `{"data":{"steps":[…dry run…],"writes":0}}` | 404, 422 `definition_invalid` | `workflows_definitions_test` |
| POST `/workspace/{workspace}/workflows/definitions/{definition}/versions/{version}/publish` | manage | — + `Idempotency-Key` | `{"data":WorkflowDefinition}` | 403 `four_eyes_required`, 409 `version_already_published`, 422 `definition_invalid`, 422 `gate_not_passed` | `workflows_definitions_publish` |
| POST `/workspace/{workspace}/workflows/definitions/{definition}/retire` | manage | — | `{"data":WorkflowDefinition}` | 404 | `workflows_definitions_retire` |
| GET `/workspace/{workspace}/workflows/runs?definition=&status=&page=&page_size=` | view | — | list of `WorkflowRun` | | `workflows_runs_list` |
| GET `/workspace/{workspace}/workflows/runs/{run}` | view | — | `{"data":WorkflowRun}` with `steps` | 404 | `workflows_runs_get` |
| POST `/workspace/{workspace}/workflows/runs/{run}/resume` | manage | `{"reason"}` + `Idempotency-Key` | `{"data":WorkflowRun}` | 409 `run_state_changed` | `workflows_runs_resume` |
| POST `/workspace/{workspace}/workflows/runs/{run}/cancel` | manage | `{"reason"}` + `Idempotency-Key` | `{"data":WorkflowRun}` | 409 `run_state_changed` | `workflows_runs_cancel` |

```jsonc
// ValidationReport
{ "ok": false, "errors": [ { "step": "s3", "code": "unknown_capability|missing_field|side_effect_without_approval|capability_unavailable", "message": "readable" } ],
  "steps": [ { "step": "s3", "owner_system": "netsuite|odoo|flow", "dangerous": true, "gate": "L14", "gate_passed": false } ] }
// WorkflowDefinition
{ "id", "key", "title", "status": "draft|published|retired", "created_by": { "user_id","name" },
  "versions": [ { "version": 1, "spec_hash", "validation": ValidationReport, "created_by", "published_by": {…}|null, "published_at": "…|null" } ] }
// WorkflowRun
{ "id", "definition_id", "version": 1, "trigger_ref", "correlation_id", "status", "current_step": 2, "created_at", "updated_at",
  "steps": [ { "step_no","step_key","capability_key","status","due_at","approval_request_id","execution": ExecutionSummary|null } ] }
```

### 11.12 Verzo tools (`apps/office/lib/voice/operation-policy.json`)

| operation_id | disposition | risk | authorization `allOf` | other |
|---|---|---|---|---|
| `reports_cross_templates_list` | execute | read | `integration.view` | `retry.mode: "reconcile_only"` |
| `reports_cross_runs_create` | execute | read | `integration.view` | `retry.mode: "reconcile_only"` |
| `reports_cross_runs_get` | execute | read | `integration.view` | `retry.mode: "reconcile_only"` |
| `reports_cross_definitions_create` | execute | write | `integration.view` | `confirmationFields: ["template_key"]` |
| `workflows_definitions_validate` | execute | read | `integration.manage` | |
| `workflows_definitions_create` | execute | write | `integration.manage` | `confirmationFields: ["title","steps"]` |
| `workflows_runs_list` | execute | read | `integration.view` | |
| `workflows_runs_get` | execute | read | `integration.view` | |

Every other new operation above is `disposition: "internal"`, explicitly including `workflows_definitions_publish`, `workflows_runs_resume`, `workflows_runs_cancel`. The catalog is regenerated by the child that edits the policy (B1.2, B4).

### 11.13 Error code registry additions (`AppError::Reason`, K14)

| status | code | where | message |
|---|---|---|---|
| 409 | `snapshot_stale` | review case decide | `This record changed in the source system after this check. Run the check again before deciding.` |
| 409 | `review_case_already_decided` | review case decide | `This case was already decided.` |
| 409 | `incident_not_verified` | incident resolve `reconciled` | `This incident has no verified read-back yet. Run Verify first, or close it by hand with a reason.` |
| 422 | `incident_not_verifiable` | incident verify | `This incident has no record to check against.` |
| 409 | `scope_in_use` | stock-scopes update | `This location pair has check history and cannot be removed.` |
| 409 | `proposal_state_changed` | replenishment submit, acknowledge | `This proposal changed state. Reload and try again.` |
| 403 | `four_eyes_required` | definition publish | `You cannot publish a version you wrote.` |
| 409 | `version_already_published` | definition publish | `This version is already published.` |
| 422 | `definition_invalid` | definition test, publish | `This workflow has problems. Fix them before you publish.` |
| 422 | `gate_not_passed` | definition publish (D-73) | `A step in this workflow is not verified yet and cannot be published.` |
| 409 | `run_state_changed` | run resume, cancel | `This run changed state. Reload and try again.` |

Existing codes reused unchanged: `four_eyes_required` (WF-06 approvals, incident resolve), `incident_already_resolved`, `execution_state_changed`, `retry_needs_grant`, `retry_already_granted`, `approval_*`. The UI maps every code to a readable sentence; raw codes never show on screen.

### 11.14 Event kinds (`workflow_events.kind`, no CHECK list)

WF-06 and WF-07 executions reuse the WF-01 kinds (§3.7). New: `expectation_met`, `expectation_late`, `expectation_failed`, `incident_verified` (B1.1); `proposal_submitted`, `proposal_superseded` (B3); `run_step`, `run_cancelled` (B4).

### 11.15 Resolved by B0a

| # | item | choice | reason |
|---|---|---|---|
| R-1 | Path param `{ws}` in the plan | `{workspace}` and resource-named params (`{case}`, `{incident}`, `{stock_check}`, `{run}`, `{proposal}`, `{definition}`, `{version}`) | K9 and every WF-01 route (§5) |
| R-2 | `GET\|PUT …/tolerances`, `GET\|PUT …/stock-scopes` | `GET` + `POST` (full replace / upsert) | K9: no PUT or PATCH in this API |
| R-3 | Tolerance and stock-scope writes permission | `integration.manage` | they are pair configuration; pair routes use manage (§5.2) |
| R-4 | `ext_records.kind`, `ext_read_runs.purpose` lists | full lists created once in B1.2 | plan gives `CHECK(...)` with no values; one list avoids five widening migrations |
| R-5 | Read-run partial message | `partial_reason` (`row_ceiling`, `page_limit`) | plan requires the UI to say why; WF-01 stores codes, not text |
| R-6 | `qty`/`amount` precision | unconstrained `numeric` | plan says exact numeric; Odoo prices are unrounded (R2-O8), so `numeric(24,8)` (K25) could truncate |
| R-7 | Case decisions in WF-04 (`explained`, `correction_requested`) vs WF-02 (`accept`, `request_correction`) | one set: `accept`, `request_correction`; WF-04 UI label "Explained" | the plan uses one shared `review_cases` table (X3) and one decide route |
| R-8 | System case moves | `review_case_decisions.decision` adds `resolve_verified`, `supersede` with `actor_kind='system'` | the decisions table is the only append-only log for cases; `workflow_events` needs an execution or request |
| R-9 | `open → resolved_verified` | allowed (not only from `awaiting_correction`) | otherwise a case stays open after the data is fixed |
| R-10 | Stale check input `snapshot_seen` | the `read_run_id` (uuid) shown to the user | plan names the field, not its type |
| R-11 | Citing records on results | `match_lines.record_ids uuid[]`, `review_cases.evidence.record_ids` | plan rule "every result cites the `ext_records` rows it used" |
| R-12 | Draft bill label | `match_lines.draft_billed boolean` | D-25 needs the "draft" label per line |
| R-13 | WF-04 "item gone" reason | `stock_check_lines.reasons` with `item_not_found`, `permission_denied` | plan test needs a readable reason; mirrors `match_lines.reasons` |
| R-14 | WF-04 window and threshold storage | pair config `stock_freshness_minutes` (60), `stock_threshold` ("0") | D-43 defaults; plan has no column; WF-01 keeps such settings in pair config |
| R-15 | Incident resolve CHECKs | WF-01 CHECKs on `resolved_by`/`note` relaxed to allow a system `reconciled` close; `manual_closed` still needs user + note; backfill `manual_closed` | plan test `wf03_late_then_met_auto_verifies` resolves with no user |
| R-16 | `category` on WF-01 incident kinds | NULL | plan defines categories only for the new failure flow; no source maps WF-01 kinds |
| R-17 | Incident resolve without `resolution_kind` | defaults to `manual_closed` | keeps the WF-01 UI body `{"note"}` valid |
| R-18 | Expectation target connection | from `external_links` by `correlation_id` | plan's identity rule; no extra column |
| R-19 | WF-06 expectation target | `target_kind='stock.warehouse.orderpoint'`, id = orderpoint id | `action_replenish` takes no external id; plan's verify re-reads the orderpoint |
| R-20 | Per-pair schedules | system job of the same kind fans out per active pair (`{pair_id}` payload) | K16 schedules are global; avoids extra tick kinds |
| R-21 | `concurrency_key` "pair" | `wf02:<pair>`, `wf06:<pair>`; WF-04 `netsuite:<connection_id>` | K17 keys are global; WF-01 uses `wf01:<pair>` |
| R-22 | WF-07 action-step execution kinds | `wf07_execute`, `wf07_reconcile` | plan says action steps create executions (W1) but names no job; mirrors WF-06/WF-01 |
| R-23 | WF-07 incident on late wait | kind `waiting_data` | plan text "`waiting_data` incident" |
| R-24 | WF-07 trigger step | library `sales_order_created`, capability `sales_order.list_changed`, gate L10 | plan has a `trigger` kind but names no trigger; the wholesale example (D-71) starts from a NetSuite SO |
| R-25 | WF-07 step library keys | `approval`, `check_stock`, `request_purchasing`, `wait_fulfillment`, `invoice_after_fulfillment`, `manual_handoff` | plan names the steps in prose only |
| R-26 | WF-07 condition operators | `eq, ne, gt, gte, lt, lte, exists`, compared in SQL | plan says "fixed operator set" without names |
| R-27 | WF-07 approval rows | `workflow_key='wf07_custom'`, `subject_ref='wf07:<run_id>:<step_no>'`, `amount_minor=0`, `currency_ref='XXX'`, `currency_decimals=0`; each side-effect step needs its own preceding approval step | `approval_requests` needs these columns; `workflow_executions.approval_request_id` is UNIQUE |
| R-28 | Wait deadline storage | `workflow_run_steps.due_at` | plan: "waits poll … with a deadline" |
| R-29 | Publish path `…/versions/{v}/publish` | under `/workflows/definitions/{definition}/versions/{version}/publish` | matches the plan's `test` route |
| R-30 | Resume/cancel permission | `integration.manage` | matches WF-01 `…/executions/{execution}/retry` |
| R-31 | Missing list routes | `workflows_definitions_list`, `workflows_stock_checks_list`, `reports_cross_definitions_list` (plan `POST\|GET`) | the plan's Office pages need lists; read-only, `integration.view` |
| R-32 | Replenishment scan / submit / acknowledge permission | `integration.approve` | plan rule "Proposals use `integration.approve`" |
| R-33 | WF-06 approval amount | `amount_minor` = SQL-rounded `est_cost`, 0 when unknown; `required_permission='integration.approve'` | D-64 one approval, no threshold policy; column is NOT NULL |
| R-34 | Approval `return` on a proposal | proposal → `rejected` | proposal states have no `returned` |
| R-35 | `customer` mapping entity | added by B4 | X6 names it; only WF-07 SO steps need it (WF-05 joins by template, D-51) |
| R-36 | Async create responses | 202 `{"data":{"<x>_id"}}` for report runs and stock checks; 202 `{"message":"ok"}` for rerun and scan | K10 id style; plan says "202 + run id" |
| R-37 | Error codes and messages in §11.13 | named here | the plan names only `snapshot_stale`; messages follow the WF-01 wording style |
| R-38 | **OPEN to parent:** WF-06 failed write | proposal stays `executing` while its execution is not terminal; plan test "`failed`, category `data`" read as expectation `failed` + incident category `data` | plan's proposal states have no failure state; B3 asks the parent before adding one |

### 11.16 Resolved by B1.1 (2026-11-21, parent approved)

| # | item | choice | reason |
|---|---|---|---|
| B1.1-A | R-17 at the database | `BEFORE INSERT OR UPDATE` trigger `workflow_incidents_default_kind`: `status = 'resolved'` with `resolution_kind` NULL and `resolved_by` set → `manual_closed` | a person's resolve written without a kind (WF-01 raw SQL, `tests/api/approvals_schema.rs`) stays valid; the system close still needs `reconciled` + `verified_at` |
| B1.1-B | Expectation target connection (changes R-18) | the execution's pair (`workflow_executions` → `approval_requests` → `integration_pairs.target_connection_id`); `external_links` only when `execution_id` is NULL | `external_links` rows exist only after a verified success, so a pending or late expectation has none |
