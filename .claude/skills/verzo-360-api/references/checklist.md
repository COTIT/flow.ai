# verzo-360-api done checklist

Select validation by change scope using [the API validation table](../../../../apps/api/README.md#local-validation). Report the checks you ran.

## Commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings  # full API gate
# Run the focused tests or full API gate specified in the validation table.
cargo sqlx prepare --check -- --all-targets  # SQL query changes
UPDATE_OPENAPI=1 cargo test --test openapi committed_spec   # intentional contract changes only
grep -ciE 'laravel|vimdesk' openapi.json                    # must print 0
```

When `openapi.json` changed, also regenerate and check the TS client from the repo root: `pnpm turbo run generate --filter=@verzo/api-client`, then `pnpm turbo run generate:check --filter=@verzo/api-client --only`.

Tests need a reachable Postgres 18 (no compose file in this repo; shared dev DB on `127.0.0.1:54320`) and `.env` with `DATABASE_URL` (runtime + tests) and `GENESY_DATABASE_URL` (macros + sqlx-cli, see `sqlx.toml`). sqlx-cli must be built with `--features postgres,rustls,sqlx-toml`.

## Review

- [ ] Test written first and seen failing (failing line quoted).
- [ ] Five buckets covered: happy, error, edge, boundary, performance.
- [ ] Layers respected: no SQL in handlers, no axum types in services.
- [ ] Every workspace-scoped query filters `workspace_id`; a cross-workspace denial test exists.
- [ ] No plaintext secret stored or logged. Constant-time comparisons.
- [ ] No `unwrap`/`expect` outside tests and boot.
- [ ] `//!` header on every new file, `///` on every new `pub` item, comments run through caveman + fuck-slop.
- [ ] Migration reversible, `.sqlx/` updated.
- [ ] Every new route has `#[utoipa::path]`, is in its module's `every_<module>_operation_is_documented` test, and `openapi.json` + the TS client are regenerated.
- [ ] `CHANGELOG.md`, `docs/api/*.md`, and root `README.md` updated when commands change.
- [ ] `ecc:rust-reviewer` and `ecc:security-reviewer` run on the diff; findings fixed or answered.
