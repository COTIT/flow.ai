# Genesy Flow current handoff

Checkpoint: 2026-11-10. **Phase 1 merged to `main` (COTIT/genesy#13, merge `e041285e7`). Staging validation in progress. Phase 2 not started.**

Read repository instructions → [MASTER_PLAN.md](MASTER_PLAN.md) → this checkpoint → actual Git/source → [VERIFICATION.md](VERIFICATION.md) and [DECISIONS.md](DECISIONS.md).

## Git

- `main` = `e041285e7` (merge of `codex/flow-foundation`: `42985896e` feature, `6f049b0d3` tag + stack locator fix, docs).
- Branch `fix/shop-image-fixture-import`: the main-branch `ci` run failed `image-prepare (web)`. Shop tests imported the API fixture as a module; the web image context excludes `apps/api` and `next build` type-checks tests (TS2307). Tests now read the fixture at runtime. Until this lands and `ci` is green, no deployable images exist for `e041285e7`.

## Staging (read 2026-11-10)

- Staging runs `fbe1fcf15` (pre-Phase-1); migration head `20261109130000`; the design route answers 404.
- One Shop instance; its `STOREFRONT_APP_KEY` maps to workspace `afm-demo` (76 products). `SHOP_DESIGN_ENABLED` unset (false).
- User approved: deploy the first green main commit to staging; validate on `afm-demo` with the real staging Shop; flag returns to false afterwards.

## How Phase 1 got here

1. A Codex agent implemented Phase 1 uncommitted; the user stopped it and a Claude parent session took over.
2. Two read-only audits checked the work against the supplied spec: nothing missing; 13 gaps fixed.
3. Two plan-first child sessions (API, frontend) with exclusive file ownership fixed them under parent review.
4. The parent regenerated SQLx, OpenAPI, client and voice artifacts, ran the full local gate, fixed two real failures, ran security + React reviews, merged.

## Open

- Land the image fix, get `main` `ci` green, deploy to staging, run the staging scenario, record evidence.
- Then decide Phase 2 readiness.

## Safety notes

- Child sessions run in their own worktrees, plan first, never push.
- Never reset/reseed shared stack state from specs; each design spec owns its workspace, key and Shop process.
- SQLx macros read `GENESY_DATABASE_URL`; `.env` points it at the shared dev DB on 54320. Point it at a disposable DB explicitly when preparing queries.
- `scripts/local-gate.sh` builds no Docker images; build `deploy/images/next.Dockerfile` locally when a change could affect the image context.
- `scripts/local-gate.sh` cleanup runs `cargo clean` and `docker builder prune -a`; expect a cold rebuild afterwards.
