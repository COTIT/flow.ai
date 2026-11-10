# Genesy Flow current handoff

Checkpoint: 2026-11-10. **Phase 1 merged and validated on staging (PASS). Flag `SHOP_DESIGN_ENABLED` is false everywhere. Phase 2 not started; it needs the user's go.**

Read repository instructions → [MASTER_PLAN.md](MASTER_PLAN.md) → this checkpoint → actual Git/source → [VERIFICATION.md](VERIFICATION.md) and [DECISIONS.md](DECISIONS.md).

## Git

- `main` includes Phase 1 (COTIT/genesy#13, merge `e041285e7`) and the shop image fix (COTIT/genesy#14, merge `dd6678bfe`).
- This checkpoint's docs and staging evidence land via branch `docs/flow-staging-validation`.

## Staging (2026-11-10)

- Runs `dd6678bfe` (API, Office, Shop, Till, Field, landing). Schema `20261109191001`. Previous: `fbe1fcf15` / `20261109130000`, with a pre-deploy dump recorded in `.deploy-state`.
- One Shop instance; its key serves `afm-demo`. Validation used that workspace (user-approved): draft → Office preview → publish → flag on → responsive checks → second revision → restore → republish → failure/isolation checks → flag restored to false. All passed; details and evidence in VERIFICATION.md.
- afm-demo now holds 3 revisions; revision 3 (fixture content) is published but hidden while the flag is off.
- Production: untouched (`fbe1fcf15`, no design migration).

## Known gaps

- `scripts/local-gate.sh` and PR CI build no Docker images; the image build runs only on `main`. Build `deploy/images/next.Dockerfile` locally when a change could touch the image context (that is how the #14 defect slipped through).
- Staging API logs no request paths, so "flag off → no design request" is proven by unit test, not runtime logs.
- Binary rollback past the design migration needs `--restore-db` or `--compatible-assistant`; never bypass the schema guard.

## Next (each needs the user's go)

1. Phase 2 (`apps/flow`): follow the MASTER_PLAN Phase 2 checklist, in small PRs (shared agent extraction / Flow auth + deploy / UI).
2. Optional: decide whether staging keeps the flag on for AFM demos (would show revision 3 until a new design is published).
3. Production rollout of the design flag is a separate, later decision.

## Working rules

- Children: Auto, Opus 5.5, high effort, own worktree, plan first; parent approves before edits; children commit locally, never push or deploy.
- Ask the user only for scope, auth/permission/payment/business rules, destructive DB ops, production/credentials/vendors/cost, security exceptions.
- SQLx macros read `GENESY_DATABASE_URL`; `.env` points it at the shared dev DB on 54320. Point it at a disposable DB explicitly when preparing queries.
- `scripts/local-gate.sh` cleanup runs `cargo clean` and `docker builder prune -a`; expect a cold rebuild afterwards.
