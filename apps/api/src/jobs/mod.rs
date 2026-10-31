//! Background job rail: jobs live in Postgres, workers claim them with SKIP LOCKED under a
//! lease, chunked jobs resume from their cursor. Contract: `docs/api/jobs.md`.

use std::time::Duration;

pub mod kinds;
pub mod repo;
pub mod runner;
pub mod scheduler;

/// How long a claim holds a job without renewal; workers renew every third of it.
pub const LEASE: Duration = Duration::from_secs(60);
/// A worker with nothing due polls again after this.
pub const IDLE: Duration = Duration::from_secs(1);
/// Longest one run (one chunk) may take; chunks aim for 10 s, cleanup batches stay well under.
pub const MAX_RUN: Duration = Duration::from_secs(10 * 60);
