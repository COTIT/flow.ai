# WF-01 frozen contracts (Wave 0)

Status: **frozen** for children C1, C2, C3a, C3b, C4a, C4b and C5. Source: `docs/integrations/PLAN.md` rev 2 with the REV 3 and REV 4 amendments. Where they conflict, the later amendment wins, and this file follows it.

- **Change rule:** a child that needs a different name, column, enum value, route or JSON field asks the parent. The child does not change it locally. The parent edits this file, bumps the version line below and tells every affected child.
- **Version:** `contracts v1.1 (2026-11-17)`: v1 plus the parent's W0 review corrections.
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
| K8 | Reversible migration pairs `YYYYMMDDHHMMSS_<name>.{up,down}.sql`, created with `sqlx migrate add -r`. The latest on base is `20261115140000_inventory_valuation`. | `.claude/skills/genesy-360-api/SKILL.md:86`; `ls migrations` |
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
| K21 | Layers per module: `mod.rs, routes.rs, handlers.rs, service.rs, repo.rs, dto.rs, model.rs`, created only as needed. API docs go in `apps/api/docs/api/<feature>.md` (the folder exists: 67 files incl. `conventions.md`, `settings.md`, `jobs.md`), with entries in `apps/api/CHANGELOG.md`. There is no repo-root `docs/api/`. | `.claude/skills/genesy-360-api/SKILL.md:28-56,91-96`; `ls apps/api/docs/api` |
| K22 | Routes are mounted in `openapi::private()` with `.merge(<module>::routes::private())`. Each module's `routes.rs` lists `routes!(handlers::x)`. | `src/openapi.rs:140-251`; `src/modules/farm_supply_corrections/routes.rs:8-15` |
| K23 | Generated client: `apps/api/openapi.json` comes from `pnpm --filter @genesy/api openapi`, then `orval` writes `packages/api-client/src/generated/*`. | `apps/api/package.json:10`; `packages/api-client/orval.config.ts:7,10`; `packages/api-client/package.json:11-12` |
| K24 | Shared UI exports are `@genesy/ui/components/*` → `src/components/*.tsx`. `Notice{variant: info\|success\|warning\|error, title, children?, action?, live?}` and `EmptyState{title, description?, action?, icon?}`. | `packages/ui/package.json:2,41-47`; `packages/ui/src/components/notice.tsx:10,35-48`; `packages/ui/src/components/empty-state.tsx:6-19` |
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

- **Shared components** in `packages/ui/src/components/business-approvals/`, imported as `@genesy/ui/components/business-approvals/<file>` (K24; O-8):
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
| O-8 | `@genesy/ui` export `"./components/*"` with a nested folder: there is no nested component folder yet (`packages/ui/src/components` has none). Node subpath `*` matches `/`, but this is unproven in this repo's TS/Next setup. C5 proves it with a build, or falls back to flat names `business-approvals-*.tsx` and asks the parent. | C5 |
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
