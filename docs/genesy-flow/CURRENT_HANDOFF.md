# Genesy Flow current handoff

Checkpoint: 2026-11-10. **Phase 1 implemented and gated; PR open for review. Do not start Phase 2 without a new go.**

Read repository instructions → [MASTER_PLAN.md](MASTER_PLAN.md) → this checkpoint → actual Git/source → [VERIFICATION.md](VERIFICATION.md) and [DECISIONS.md](DECISIONS.md).

## Git

- Branch `codex/flow-foundation`, base `fbe1fcf15` (unchanged `main`).
- `42985896e` feat: Phase 1 storefront home design foundation (Codex implementation + audit fixes + regenerated artifacts).
- `6f049b0d3` fix: OpenAPI tag convention + stack spec revision locators.
- Pushed with a PR to `main`. No merge, no deploy.

## How this got here

1. A Codex agent implemented Phase 1 uncommitted; the user stopped it and a Claude parent session took over.
2. Two read-only audits checked the work against the supplied spec: nothing missing; 13 gaps fixed.
3. Two plan-first child sessions (API, frontend) with exclusive file ownership fixed them under parent review.
4. The parent regenerated SQLx, OpenAPI, client and voice artifacts (authority hashes refreshed only for reviewed files whose auth gates were unchanged), ran the full local gate, fixed the two real failures, and ran security + React reviews.

## Open

- Full-stack storefront spec rerun after `6f049b0d3`: 2 passed.
- CI on the PR.
- Later, separately approved: staging rollout with `SHOP_DESIGN_ENABLED=true` for one controlled workspace; Phase 2 (Flow app).

## Safety notes

- Never reset/reseed shared stack state from specs; each design spec owns its workspace, key and Shop process.
- SQLx macros read `GENESY_DATABASE_URL`; `.env` points it at the shared dev DB on 54320. Point it at a disposable DB explicitly when preparing queries.
- `scripts/local-gate.sh` cleanup runs `cargo clean` and `docker builder prune -a`; expect a cold rebuild afterwards.
