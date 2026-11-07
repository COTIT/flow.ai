# Background jobs (the job rail)

No HTTP routes. This is the contract for code that runs work in the background: batch 7's imports, exports, feeds and webhooks, the final batch's F8 (webhook inbox, Mollie), and batch 8's due-date reminders. Code: `src/jobs/`.

Laravel ran this on Redis queues under Horizon. Two problems there: `retry_after` (90 s) was below the job timeouts, so long jobs ran twice, and a nightly `FlushRedis` wiped whatever was queued. Genesy keeps jobs in Postgres.

## How a job runs

1. **Enqueue.** Something calls `jobs::repo::enqueue` (or `enqueue_coalesced`) inside its own transaction. The job exists only if that transaction commits.
2. **Claim.** A worker takes the next due job with `FOR UPDATE SKIP LOCKED`, so two workers never get the same row, even across blue/green colors. The claim:
   - sets `status = running`;
   - writes a fresh lease token into `locked_by`;
   - sets `locked_until = now() + 60 s`.

   It runs in two statements:
   - **Fast path:** the 32 oldest due jobs, and the first one whose workspace isn't at its cap and whose concurrency key isn't held.
   - **Fallback:** runs only when there were due jobs and all 32 were blocked or locked by other claims (a capped tenant's backlog at the head). An empty queue never reaches it. It walks each workspace's first due job through `jobs_queued_idx`, so the backlog costs one index probe. Its `ORDER BY` leads with `workspace_id`, so only that index can serve it: with stale statistics the planner used to scan the global index past the whole backlog (268 ms per claim, measured).
3. **Run.** The handler runs in its own task. A side task renews the lease every 20 s. The run is stopped and retried when:
   - a renewal finds the lease gone;
   - no renewal has succeeded for a whole lease (60 s), since handlers without a checkpoint, like cleanup, have no fencing;
   - it runs past `MAX_RUN` (10 min).
4. **Outcome.** Every outcome is keyed on the token:

   | Handler result | What happens |
   |---|---|
   | `Ok(Step::Done)` | `succeeded` |
   | `Ok(Step::More)` | back to `queued` at the end of the line, attempts reset |
   | permanent error (`AppError::is_permanent`: validation, not found, forbidden, bad request) | `failed` at once |
   | any other error, or a panic | queued again after a backoff (10 s doubling, 1 h max, ±20 % jitter); `failed` after `max_attempts` consecutive failures |
5. **Reaper.** A worker that dies or stalls stops renewing. Its lease runs out, and the scheduler's tick puts the job back on the queue with the same backoff. When the stalled worker wakes up, its token no longer matches, so it can't save progress or set an outcome.

## Enqueue API (`jobs::repo`)

```rust
let mut conn = /* your transaction or a pool connection */;
let id = jobs::repo::enqueue(&mut conn, &NewJob {
    workspace_id: Some(ws),                       // None only for system jobs (cleanup)
    payload: json!({ "import_id": id }),          // JSON object, max 64 KiB, carries the cursor
    concurrency_key: Some(format!("product_import:{ws}")),
    dedupe_key: None,
    created_by: Some(user_id),
    ..NewJob::new(kinds::PRODUCT_IMPORT)
}).await?;
```

| Field | Meaning | Default |
|---|---|---|
| `kind` | One of `jobs::kinds::KINDS` | required |
| `workspace_id` | Tenant. Deleting the workspace deletes its jobs. `None` = system job, outside every cap | `None` |
| `payload` | JSON object, ≤ 64 KiB | `{}` |
| `priority` | Higher runs first | 0 |
| `max_attempts` | Consecutive failures before `failed`, 1..100 | 5 |
| `concurrency_key` | At most one **running** job per key (unique index). Others wait queued | none |
| `dedupe_key` | At most one **queued or running** job per key **within the job's workspace** (system jobs, workspace `None`, share one scope). `enqueue` returns that live job's id instead of inserting; the same key in another workspace is another job | none |
| `created_by` | User who asked for it (polling routes filter on it) | none |

`enqueue_coalesced(conn, job) -> Option<Uuid>` (workspace jobs only) skips the insert when a job of the same kind and workspace is queued and **hasn't started**: that job will also see your change. A job that started doesn't count, because it may already have passed your change. That covers a running job, and also one queued between chunks with its cursor moved on. Pair it with a `concurrency_key`, so the new job waits for the running one.

Call it inside the transaction that makes the change:
- an advisory lock per kind and workspace serialises concurrent callers;
- a share lock on the queued job keeps workers from claiming it before your change commits.

The warehouse stock sweep uses this.

## Writing a handler

A new kind needs three things in `jobs::kinds`:
- a `const`;
- an entry in `KINDS`;
- a match arm in `dispatch`.

Handlers get `Ctx` and the `Claimed` job. Specific invoice delivery also receives the configured application services using the job pool; jobs contain only a delivery id. Credentials stay in runtime configuration. The hourly prune additionally removes resolved assistant/delivery audit after 90 days and up to 100 old cleaned session contexts without unresolved references.

**Rules:**
- **One connection at a time.** The job pool has one connection per worker. Holding two at once can starve the other workers.
- **Short chunks.** One chunk must finish within ~10 s. On deploy, a draining instance gives running chunks 8 s, starting at the signal; after that their leases expire and the other color resumes them. Job pool connections cap a statement at 5 min and a transaction at 10 min, and `MAX_RUN` stops a run at 10 min.
- **Chunk contract.** Do one chunk in one transaction:
  1. `SET LOCAL statement_timeout = '30s'` (the job pool has no default);
  2. read the next rows after the cursor in the payload (keyset, `> cursor ORDER BY key LIMIT n`);
  3. write with set-based SQL (`UNNEST`, `INSERT … SELECT`, `COPY`);
  4. call `job.checkpoint(&mut *tx, &new_payload, done, total)`. If it returns `false`, the lease is gone: return an error **without committing**, so the chunk rolls back;
  5. commit, then return `Step::More`. When there's nothing left, return `Step::Done`.
- **Idempotent.** A crash replays at most one chunk. Rows a chunk already did must be skipped or upserted (`ON CONFLICT`), never written twice.
- **Lock order** (risk register): workspace advisory locks, then the document row, then products by id, then stock slots (`stock::repo::lock_all`). Inserts come last.
- **Work streams, not work items.** Keep the jobs table to a handful of rows per tenant. Fan-out (one row per webhook delivery, per imported line) goes into the domain's own table, drained by one chunked job per stream. A tenant with 100k queued job rows makes every claim scan past them (see "Load" below).
- **Errors.** Return a permanent `AppError` for input that will never work, and anything else for transient trouble. `last_error` keeps the first 1000 characters. It's internal, so map it to a safe message before any route shows it.

## Schedules

`jobs::kinds::SCHEDULES` lists recurring jobs as `Schedule { name, kind, every }`, with `every` either `Every::Minutes(n)` (UTC multiples of `n` minutes) or `Every::DailyAt { hour, minute }` (UTC; no DST handling, so a nightly job shifts one hour twice a year).

How a tick works:
- Every instance ticks every 30 s. A tick takes `pg_try_advisory_xact_lock`; when another instance holds it, the tick does nothing.
- The winner inserts each schedule's latest due slot into `scheduled_runs` (primary key `(name, slot)`), and enqueues the new ones in the same transaction with `dedupe_key = schedule:<name>`. So a slow job is never stacked by the next slot.
- A slot missed while every instance was down runs once on the next tick.

| Schedule | Every | What |
|---|---|---|
| `close_stuck_sends` | 5 min | email sends left open for 10 min become `unknown` |
| `prune` | 60 min | expired tokens, invites, login quotas, stuck sends, jobs (succeeded/cancelled after 7 days, failed after 30), slots after 30 days |
| `file_sweep` | 60 min | file rows a crash left pending or deleting (only with a bucket) |
| `assistant_cleanup` | 1 min | expire transient drafts/claims, remove due provider conversations after confirmed deletion |
| `invoice_delivery_sweep` | 5 min | reconcile typed mail evidence or mark interrupted attempts unknown; never resend |


## Kinds

| Kind | Workspace | Keys | Payload | Enqueued by |
|---|---|---|---|---|
| `close_stuck_sends`, `prune`, `file_sweep` | none | `dedupe_key = schedule:<name>` | `{}` | the scheduler |
| `invoice_delivery` | yes | `dedupe_key = invoice_delivery:<delivery>` | `{delivery_id}` only | invoice numbering/resend transaction |
| `assistant_cleanup`, `invoice_delivery_sweep` | none | `dedupe_key = schedule:<name>` | `{}` | the scheduler |
| `stock_seed` | yes | `concurrency_key = stock_seed:<ws>`, coalesced | `{after, done}` cursor | warehouse create, an update that turns a warehouse into a store or the main one, set-default |

`stock_seed` chunks hold at most 50,000 stock rows: 5000 products, fewer when there are more than 10 target warehouses. Each chunk locks the products it updates in id order, then inserts the stock rows in (product, warehouse) order, the same order as `stock::repo::lock_all`.

`stock_seed` replaces Laravel's nightly `products:ensure-warehouse-stock-entries`. Each chunk covers 5000 live `has_stock` products and writes:
- zero warehouse-level rows in the main warehouse and every store (`ON CONFLICT DO NOTHING`), with no ledger rows;
- the main warehouse as default where a product has none. A chosen default stays, as with product create.

A workspace without a main warehouse gets nothing (Laravel skipped it). Re-saving a store, which the SPA does on every save, enqueues nothing.

## Limits and configuration

| Setting | Default | Meaning |
|---|---|---|
| `DATABASE_JOB_CONNECTIONS` | 4 | Job pool size and worker count per instance. Deploy: prod 4, previews 1 (`GENESY_DATABASE_JOB_CONNECTIONS`) |
| `JOB_MAX_RUNNING_PER_WORKSPACE` | 2 | Soft cap: the claim skips a workspace already running this many. Two claims at the same instant can pass it by one; never rely on it for correctness (use `concurrency_key`) |

Two pools serve the rail:
- **Job pool:** the workers, each holding one connection. Caps: statement 5 min, transaction 10 min.
- **One-connection control pool:** scheduler ticks and lease renewals, so a pool full of chunks can't stop renewals. Caps: statement 15 s, lock 5 s. The reaper skips rows a chunk transaction still holds.

An instance therefore opens `DATABASE_MAX_CONNECTIONS + DATABASE_JOB_CONNECTIONS + 2` connections. `deploy/deploy.sh` checks `2 × (request + job + 2) ≤ GENESY_PG_CONNECTION_LIMIT`, because both blue/green colors run at once.

Blue/green: the claim only takes `jobs::kinds::KINDS`, so the older color never runs a kind it doesn't know. A new kind waits queued until the new color claims it. The `kind` column has no CHECK list for the same reason.

## Load (tests/api/load_jobs.rs)

`cargo test --release --test load_jobs -- --ignored --nocapture --test-threads=1` against local Postgres 18 (Docker on a dev Mac, 2026-10-23):

| Test | Target | Measured |
|---|---|---|
| L1 exactly once, 50k jobs, 2 pools × 4 workers, one pool frozen mid-run (renewals stop, handlers wake as zombies) | every effect once | every effect once |
| L1 claim, one tenant with 100k queued at its cap | p95 < 10 ms | p50 1.1 ms, p95 2.0 ms |
| L1 small tenant's job start while another has 100k queued | < 2 s | p50 24 ms, max 424 ms |
| L1 throughput, 50k trivial jobs, 2 pools × 4 workers | 1,000 jobs/s | 1,198 jobs/s (740–980 before the review fixes: dropping the `locked_until` index made renewals HOT, and the fallback no longer runs on an empty queue). The test asserts a 500 floor as a regression guard; runs on a busy dev Mac vary |
| L2 two schedulers over 1,000 slots | each slot once | each once, 0.37 s |

Invoice delivery uses a durable domain claim in addition to the rail lease. A retried/reclaimed job cannot repeat rendering or SMTP after that claim was consumed. Interrupted attempts become unknown; known accepted/blocked log evidence resolves them without another transport call. Queued intent can run after restart. See `invoices.md` for receipt and verification rules.
