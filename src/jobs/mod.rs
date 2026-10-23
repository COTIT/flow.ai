//! Background job rail: jobs live in Postgres, workers claim them with SKIP LOCKED under a
//! lease, chunked jobs resume from their cursor. Contract: `docs/api/jobs.md`.

pub mod repo;
