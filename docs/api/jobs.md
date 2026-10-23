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
   - **Fallback:** runs only when all 32 are blocked, which happens when a capped tenant's backlog sits at the head. It walks each workspace's first due job through `jobs_queued_idx`, so the backlog costs one index probe. Its `ORDER BY` leads with `workspace_id`, so only that index can serve it: with stale statistics the planner used to scan the global index past the whole backlog (268 ms per claim, measured).
3. **Run.** The handler runs in its own task. A side task renews the lease every 20 s.
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
| `dedupe_key` | At most one **queued or running** job per key. `enqueue` returns the live job's id instead of inserting | none |
| `created_by` | User who asked for it (polling routes filter on it) | none |

`enqueue_coalesced(conn, job) -> Option<Uuid>` skips the insert when a job of the same kind and workspace is still **queued**: that job will also see your change. A **running** one doesn't count, because it may already have passed your change. Pair it with a `concurrency_key`, so the new job waits for the running one. The warehouse stock sweep uses this.

## Writing a handler

A new kind needs three things in `jobs::kinds`:
- a `const`;
- an entry in `KINDS`;
- a match arm in `dispatch`.

Handlers get `Ctx` (the job pool and, when configured, the file bucket) and the `Claimed` job.

**Rules:**
- **One connection at a time.** The job pool has one connection per worker. Holding two at once can starve the other workers.
- **Short chunks.** One chunk must finish within ~10 s. On deploy, a draining instance gives running chunks 8 s; after that their leases expire and the other color resumes them.
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

## Kinds

| Kind | Workspace | Keys | Payload | Enqueued by |
|---|---|---|---|---|
| `close_stuck_sends`, `prune`, `file_sweep` | none | `dedupe_key = schedule:<name>` | `{}` | the scheduler |
| `stock_seed` | yes | `concurrency_key = stock_seed:<ws>`, coalesced | `{after, done}` cursor | warehouse create, an update that turns a warehouse into a store or the main one, set-default |

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
- **Job pool:** the workers, each holding one connection.
- **One-connection control pool:** scheduler ticks and lease renewals, so a pool full of chunks can't stop renewals.

An instance therefore opens `DATABASE_MAX_CONNECTIONS + DATABASE_JOB_CONNECTIONS + 2` connections. `deploy/deploy.sh` checks `2 × (request + job + 2) ≤ GENESY_PG_CONNECTION_LIMIT`, because both blue/green colors run at once.

Blue/green: the claim only takes `jobs::kinds::KINDS`, so the older color never runs a kind it doesn't know. A new kind waits queued until the new color claims it. The `kind` column has no CHECK list for the same reason.

## Load (tests/load_jobs.rs)

`cargo test --release --test load_jobs -- --ignored --nocapture --test-threads=1` against local Postgres 18 (Docker on a dev Mac, 2026-10-23):

| Test | Target | Measured |
|---|---|---|
| L1 exactly once, 50k jobs, 2 pools × 4 workers, one pool frozen mid-run (renewals stop, handlers wake as zombies) | every effect once | every effect once |
| L1 claim, one tenant with 100k queued at its cap | p95 < 10 ms | p50 1.2–1.6 ms, p95 2.7–3.4 ms |
| L1 small tenant's job start while another has 100k queued | < 2 s | max 7–103 ms |
| L1 throughput, 50k trivial jobs, 2 pools × 4 workers | 1,000 jobs/s | **740–980 jobs/s, below target**. Each job is two status updates across five partial indexes. The test asserts a 500 floor as a regression guard. Real jobs are chunks of thousands of rows; claiming several jobs per statement is the next step if it ever matters |
| L2 two schedulers over 1,000 slots | each slot once | each once, 0.4–0.7 s |
