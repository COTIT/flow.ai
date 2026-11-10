# Genesy Flow current handoff

Checkpoint: 2026-11-10. **Phase 1 merged and staging-validated (flag false). Phase 2 authorized: PR 2A (shared voice package) on review; 2B/2C not started.**

Read repository instructions → [MASTER_PLAN.md](MASTER_PLAN.md) → this checkpoint → actual Git/source → [VERIFICATION.md](VERIFICATION.md) and [DECISIONS.md](DECISIONS.md).

## Git

- `main` includes Phase 1 (COTIT/genesy#13, merge `e041285e7`) and the shop image fix (COTIT/genesy#14, merge `dd6678bfe`).
- Staging evidence merged in COTIT/genesy#15 (`b70b909a2`).
- PR 2A: branch `flow/2a-shared-voice`. `packages/voice` (`@genesy/voice`) holds the voice client engine (`components/provider.tsx`), reusable cards/results/controls and pure `lib/*` contracts; Office keeps its conversation screen, secure form, voice pages, all server code (`lib/voice` server modules, `app/api/voice/*`), generated catalog/policy and translations. See DECISIONS.md for the boundary and VERIFICATION.md for evidence.

## Staging (2026-11-10)

- Runs `dd6678bfe` (API, Office, Shop, Till, Field, landing). Schema `20261109191001`. Previous: `fbe1fcf15` / `20261109130000`, with a pre-deploy dump recorded in `.deploy-state`.
- One Shop instance; its key serves `afm-demo`. Validation used that workspace (user-approved): draft → Office preview → publish → flag on → responsive checks → second revision → restore → republish → failure/isolation checks → flag restored to false. All passed; details and evidence in VERIFICATION.md.
- afm-demo now holds 3 revisions; revision 3 (fixture content) is published but hidden while the flag is off.
- Production: untouched (`fbe1fcf15`, no design migration).

## Known gaps

- `scripts/local-gate.sh` and PR CI build no Docker images; the image build runs only on `main`. Build `deploy/images/next.Dockerfile` locally when a change could touch the image context (that is how the #14 defect slipped through).
- Staging API logs no request paths, so "flag off → no design request" is proven by unit test, not runtime logs.
- Binary rollback past the design migration needs `--restore-db` or `--compatible-assistant`; never bypass the schema guard.

## Next

1. Merge PR 2A once CI is green (user go given).
2. PR 2B (start only after 2A merges): `apps/flow` shell on its own origin. Planned: Flow server proxies `/api/voice/*` to Office internally; Flow sign-in uses the existing API login on its own origin (origin-local bearer like Office, no shared cookies, no tokens in URLs); move the `voice` translation namespace into `packages/i18n` messages; worklet assets (`prepare-voice-audio`) for Flow; deploy/compose/Traefik/CSP for `flow.genesy.app`; Docker image built locally. Any auth-policy change goes to the user first.
3. PR 2C: Flow two-pane assistant UI from `useVoice()` + shared cards, storefront revision preview reusing `StorefrontPage` and the Phase 1 API (design ops stay `internal` for voice).
2. Optional: decide whether staging keeps the flag on for AFM demos (would show revision 3 until a new design is published).
3. Production rollout of the design flag is a separate, later decision.

## Working rules

- Children: Auto, Opus 5.5, high effort, own worktree, plan first; parent approves before edits; children commit locally, never push or deploy.
- Ask the user only for scope, auth/permission/payment/business rules, destructive DB ops, production/credentials/vendors/cost, security exceptions.
- SQLx macros read `GENESY_DATABASE_URL`; `.env` points it at the shared dev DB on 54320. Point it at a disposable DB explicitly when preparing queries.
- `scripts/local-gate.sh` cleanup runs `cargo clean` and `docker builder prune -a`; expect a cold rebuild afterwards.
