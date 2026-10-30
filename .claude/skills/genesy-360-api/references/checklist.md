# genesy-360-api done checklist

Run from `apps/api`. Paste the result of each command in the step summary.

## Commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo sqlx prepare --check -- --all-targets
```

Tests need a reachable Postgres 18 (no compose file in this repo yet) and `.env` with `DATABASE_URL` (runtime + tests) and `GENESY_DATABASE_URL` (macros + sqlx-cli, see `sqlx.toml`). sqlx-cli must be built with `--features postgres,rustls,sqlx-toml`.

## Review

- [ ] Test written first and seen failing (failing line quoted).
- [ ] Five buckets covered: happy, error, edge, boundary, performance.
- [ ] Layers respected: no SQL in handlers, no axum types in services.
- [ ] Every workspace-scoped query filters `workspace_id`; a cross-workspace denial test exists.
- [ ] No plaintext secret stored or logged. Constant-time comparisons.
- [ ] No `unwrap`/`expect` outside tests and boot.
- [ ] `//!` header on every new file, `///` on every new `pub` item, comments run through caveman + fuck-slop.
- [ ] Migration reversible, `.sqlx/` updated.
- [ ] `CHANGELOG.md`, `docs/api/*.md`, and root `README.md` updated when commands change.
- [ ] `ecc:rust-reviewer` and `ecc:security-reviewer` run on the diff; findings fixed or answered.
